// holoagent — JVMTI agent that streams events to Holo over an abstract
// Unix socket bound at @holoagent-<pid>.
//
// Wire format (big-endian):
//   [u8 kind][u32 payload_len][payload]
//
// Event kinds:
//   0x01 GC pause      payload: [i64 ts_ns][u32 duration_us]              (12 bytes)
//   0x02 MemorySample  payload: [i64 ts_ns][u32 rss_kb]
//                               [u32 java_heap_kb][u32 native_heap_kb]    (20 bytes)
//                      future fields append more u32s; readers use
//                      payload_len to know what's present.
//
// We avoid the C++ standard library entirely (no <mutex>, <thread>, <vector>,
// <chrono>, <atomic>): linking libc++ statically into a JVMTI agent leaks
// weak operator-new symbols that the dynamic linker may resolve against the
// host process's own libc++, causing late SIGSEGVs. POSIX primitives keep
// the agent self-contained.

#include <jni.h>
#include "jvmti.h"

#include <android/log.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#define TAG "holoagent"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO,  TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, TAG, __VA_ARGS__)

namespace {

constexpr size_t QUEUE_CAP = 4096;
constexpr uint8_t KIND_GC = 0x01;
constexpr uint8_t KIND_MEMORY = 0x02;

// `value` carries the kind-specific u32: GC duration_us, or RSS in KB.
// `java_heap_kb` and `native_heap_kb` are unused for GC events.
struct Event {
    uint8_t kind;
    int64_t ts_ns;
    uint32_t value;
    uint32_t java_heap_kb;
    uint32_t native_heap_kb;
};

// Fixed-size ring buffer guarded by a pthread mutex. drop-oldest on overflow.
pthread_mutex_t g_queue_mu = PTHREAD_MUTEX_INITIALIZER;
Event g_queue[QUEUE_CAP];
size_t g_queue_head = 0;   // index of oldest valid element
size_t g_queue_count = 0;

int g_started = 0;          // guarded by g_queue_mu
int64_t g_gc_start_ns = 0;  // touched only from JVMTI GC callbacks (one at a time)

int64_t now_ns() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

void enqueue(const Event& e) {
    pthread_mutex_lock(&g_queue_mu);
    if (g_queue_count >= QUEUE_CAP) {
        g_queue_head = (g_queue_head + 1) % QUEUE_CAP;
        g_queue_count--;
    }
    size_t tail = (g_queue_head + g_queue_count) % QUEUE_CAP;
    g_queue[tail] = e;
    g_queue_count++;
    pthread_mutex_unlock(&g_queue_mu);
}

// Move up to `max` queued events into `out`. Returns count moved.
size_t drain(Event* out, size_t max) {
    pthread_mutex_lock(&g_queue_mu);
    size_t n = g_queue_count < max ? g_queue_count : max;
    for (size_t i = 0; i < n; i++) {
        out[i] = g_queue[(g_queue_head + i) % QUEUE_CAP];
    }
    g_queue_head = (g_queue_head + n) % QUEUE_CAP;
    g_queue_count -= n;
    pthread_mutex_unlock(&g_queue_mu);
    return n;
}

void JNICALL OnGCStart(jvmtiEnv*) {
    g_gc_start_ns = now_ns();
}

void JNICALL OnGCFinish(jvmtiEnv*) {
    int64_t start = g_gc_start_ns;
    if (start == 0) return;
    int64_t end = now_ns();
    Event e{KIND_GC, end, (uint32_t)((end - start) / 1000), 0, 0};
    enqueue(e);
}

// Read /proc/self/statm to get the resident set size in KB. statm format is:
//   "size resident shared text lib data dt" (pages, space-separated). We only
//   need the second field. Returns 0 on any read/parse failure — the host
//   sees the missing sample and the chart simply skips a tick.
uint32_t read_rss_kb() {
    int fd = ::open("/proc/self/statm", O_RDONLY | O_CLOEXEC);
    if (fd < 0) return 0;
    char buf[128];
    ssize_t n = ::read(fd, buf, sizeof(buf) - 1);
    ::close(fd);
    if (n <= 0) return 0;
    buf[n] = '\0';
    // Skip the first field, then read the second.
    char* p = buf;
    while (*p && *p != ' ') ++p;
    if (*p != ' ') return 0;
    ++p;
    long pages = strtol(p, nullptr, 10);
    if (pages <= 0) return 0;
    long page_kb = sysconf(_SC_PAGESIZE) / 1024;
    if (page_kb <= 0) return 0;
    return (uint32_t)(pages * page_kb);
}

// JNI handles for the per-tick heap reads. Initialized once in `sampler_loop`
// after AttachCurrentThreadAsDaemon; stay valid for the agent's lifetime as
// global refs. If attach or any lookup fails, `g_jni_ok` stays false and the
// sampler emits RSS-only memory events (12-byte payload).
struct JniHeap {
    JNIEnv* env;
    jclass runtime_cls;
    jobject runtime_obj;
    jmethodID total_mid;
    jmethodID free_mid;
    jclass debug_cls;
    jmethodID native_alloc_mid;
};

bool init_jni_heap(JavaVM* vm, JniHeap* h) {
    *h = JniHeap{};
    if (vm == nullptr) return false;
    if (vm->AttachCurrentThreadAsDaemon(&h->env, nullptr) != JNI_OK || h->env == nullptr) {
        LOGE("AttachCurrentThreadAsDaemon failed");
        return false;
    }
    JNIEnv* env = h->env;
    jclass local_runtime = env->FindClass("java/lang/Runtime");
    jclass local_debug   = env->FindClass("android/os/Debug");
    if (local_runtime == nullptr || local_debug == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("FindClass for Runtime/Debug failed");
        return false;
    }
    h->runtime_cls = (jclass)env->NewGlobalRef(local_runtime);
    h->debug_cls   = (jclass)env->NewGlobalRef(local_debug);
    env->DeleteLocalRef(local_runtime);
    env->DeleteLocalRef(local_debug);

    jmethodID get_runtime_mid = env->GetStaticMethodID(h->runtime_cls, "getRuntime", "()Ljava/lang/Runtime;");
    h->total_mid = env->GetMethodID(h->runtime_cls, "totalMemory", "()J");
    h->free_mid  = env->GetMethodID(h->runtime_cls, "freeMemory",  "()J");
    h->native_alloc_mid = env->GetStaticMethodID(h->debug_cls, "getNativeHeapAllocatedSize", "()J");
    if (get_runtime_mid == nullptr || h->total_mid == nullptr ||
        h->free_mid == nullptr || h->native_alloc_mid == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("GetMethodID lookup failed");
        return false;
    }

    jobject local_rt = env->CallStaticObjectMethod(h->runtime_cls, get_runtime_mid);
    if (env->ExceptionCheck() || local_rt == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("Runtime.getRuntime() failed");
        return false;
    }
    h->runtime_obj = env->NewGlobalRef(local_rt);
    env->DeleteLocalRef(local_rt);
    return true;
}

// Read Java + native heap via cached JNI handles. On any exception, clear and
// fall back to 0 for that field (the chart treats absent values as a skipped
// tick rather than a real zero).
void read_jni_heap(JniHeap* h, uint32_t* java_kb, uint32_t* native_kb) {
    JNIEnv* env = h->env;
    jlong total = env->CallLongMethod(h->runtime_obj, h->total_mid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); total = 0; }
    jlong free_ = env->CallLongMethod(h->runtime_obj, h->free_mid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); free_ = 0; }
    jlong used = total - free_;
    if (used < 0) used = 0;
    jlong native_bytes = env->CallStaticLongMethod(h->debug_cls, h->native_alloc_mid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); native_bytes = 0; }
    *java_kb   = (uint32_t)(used / 1024);
    *native_kb = (uint32_t)(native_bytes / 1024);
}

void* sampler_loop(void* arg) {
    JavaVM* vm = (JavaVM*)arg;
    JniHeap heap;
    if (!init_jni_heap(vm, &heap)) {
        LOGE("sampler stopping: JNI heap setup failed");
        return nullptr;
    }
    for (;;) {
        Event e;
        e.kind = KIND_MEMORY;
        e.ts_ns = now_ns();
        e.value = read_rss_kb();
        read_jni_heap(&heap, &e.java_heap_kb, &e.native_heap_kb);
        enqueue(e);
        ::usleep(1000 * 1000);
    }
}

void put_be32(uint8_t* p, uint32_t v) {
    p[0] = (v >> 24) & 0xff;
    p[1] = (v >> 16) & 0xff;
    p[2] = (v >> 8)  & 0xff;
    p[3] = v & 0xff;
}

void put_be64(uint8_t* p, int64_t v) {
    uint64_t u = (uint64_t)v;
    for (int i = 0; i < 8; ++i) p[i] = (u >> (56 - 8 * i)) & 0xff;
}

bool send_all(int fd, const uint8_t* buf, size_t n) {
    while (n > 0) {
        ssize_t w = ::send(fd, buf, n, MSG_NOSIGNAL);
        if (w <= 0) return false;
        buf += w;
        n -= (size_t)w;
    }
    return true;
}

bool send_event(int fd, const Event& e) {
    // GC:     [u8 kind][u32 len=12][i64 ts_ns][u32 duration_us]
    // Memory: [u8 kind][u32 len=20][i64 ts_ns][u32 rss_kb]
    //                  [u32 java_heap_kb][u32 native_heap_kb]
    if (e.kind == KIND_MEMORY) {
        uint8_t buf[1 + 4 + 20];
        buf[0] = e.kind;
        put_be32(buf + 1, 20);
        put_be64(buf + 5, e.ts_ns);
        put_be32(buf + 13, e.value);
        put_be32(buf + 17, e.java_heap_kb);
        put_be32(buf + 21, e.native_heap_kb);
        return send_all(fd, buf, sizeof(buf));
    }
    uint8_t buf[1 + 4 + 12];
    buf[0] = e.kind;
    put_be32(buf + 1, 12);
    put_be64(buf + 5, e.ts_ns);
    put_be32(buf + 13, e.value);
    return send_all(fd, buf, sizeof(buf));
}

int bind_listen() {
    int srv = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (srv < 0) {
        LOGE("socket() failed: %d", errno);
        return -1;
    }

    char name[64];
    int name_len = snprintf(name, sizeof(name), "holoagent-%d", getpid());

    sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    addr.sun_path[0] = '\0';
    memcpy(addr.sun_path + 1, name, (size_t)name_len);
    socklen_t addr_len = offsetof(sockaddr_un, sun_path) + 1 + name_len;

    if (::bind(srv, (sockaddr*)&addr, addr_len) < 0) {
        LOGE("bind(@%s) failed: %d", name, errno);
        ::close(srv);
        return -1;
    }
    if (::listen(srv, 1) < 0) {
        LOGE("listen() failed: %d", errno);
        ::close(srv);
        return -1;
    }
    LOGI("listening @%s", name);
    return srv;
}

void serve(int srv) {
    Event batch[QUEUE_CAP];
    while (true) {
        int client = ::accept(srv, nullptr, nullptr);
        if (client < 0) {
            if (errno == EINTR) continue;
            LOGE("accept() failed: %d", errno);
            return;
        }
        for (;;) {
            size_t n = drain(batch, QUEUE_CAP);
            bool ok = true;
            for (size_t i = 0; i < n; ++i) {
                if (!send_event(client, batch[i])) { ok = false; break; }
            }
            if (!ok) break;
            ::usleep(100 * 1000);
        }
        ::close(client);
    }
}

void* writer_loop(void*) {
    int srv = bind_listen();
    if (srv < 0) return nullptr;
    serve(srv);
    ::close(srv);
    return nullptr;
}

jvmtiError install_gc_tracker(jvmtiEnv* jvmti) {
    jvmtiCapabilities caps;
    memset(&caps, 0, sizeof(caps));
    caps.can_generate_garbage_collection_events = 1;
    if (jvmtiError err = jvmti->AddCapabilities(&caps); err != JVMTI_ERROR_NONE) {
        return err;
    }

    jvmtiEventCallbacks cbs;
    memset(&cbs, 0, sizeof(cbs));
    cbs.GarbageCollectionStart  = OnGCStart;
    cbs.GarbageCollectionFinish = OnGCFinish;
    if (jvmtiError err = jvmti->SetEventCallbacks(&cbs, sizeof(cbs)); err != JVMTI_ERROR_NONE) {
        return err;
    }

    if (jvmtiError err = jvmti->SetEventNotificationMode(
            JVMTI_ENABLE, JVMTI_EVENT_GARBAGE_COLLECTION_START, nullptr);
        err != JVMTI_ERROR_NONE) {
        return err;
    }
    return jvmti->SetEventNotificationMode(
        JVMTI_ENABLE, JVMTI_EVENT_GARBAGE_COLLECTION_FINISH, nullptr);
}

jint attach(JavaVM* vm) {
    jvmtiEnv* jvmti = nullptr;
    if (vm->GetEnv((void**)&jvmti, JVMTI_VERSION_1_2) != JNI_OK || jvmti == nullptr) {
        LOGE("GetEnv failed");
        return JNI_ERR;
    }
    if (jvmtiError err = install_gc_tracker(jvmti); err != JVMTI_ERROR_NONE) {
        LOGE("install_gc_tracker failed: %d", err);
        return JNI_ERR;
    }
    // attach-agent can fire more than once; only spawn the writer once.
    pthread_mutex_lock(&g_queue_mu);
    int already = g_started;
    g_started = 1;
    pthread_mutex_unlock(&g_queue_mu);
    if (!already) {
        pthread_t writer_tid;
        if (pthread_create(&writer_tid, nullptr, writer_loop, nullptr) == 0) {
            pthread_detach(writer_tid);
        } else {
            LOGE("pthread_create(writer) failed: %d", errno);
        }
        pthread_t sampler_tid;
        if (pthread_create(&sampler_tid, nullptr, sampler_loop, vm) == 0) {
            pthread_detach(sampler_tid);
        } else {
            LOGE("pthread_create(sampler) failed: %d", errno);
        }
    }
    return JNI_OK;
}

}  // namespace

extern "C" JNIEXPORT jint JNICALL
Agent_OnAttach(JavaVM* vm, char* /*options*/, void* /*reserved*/) {
    return attach(vm);
}

extern "C" JNIEXPORT jint JNICALL
Agent_OnLoad(JavaVM* vm, char* /*options*/, void* /*reserved*/) {
    return attach(vm);
}
