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
//   0x03 CpuSample     payload: [i64 ts_ns][u32 cpu_centi_percent]
//                                [u32 num_threads]                        (16 bytes)
//                      cpu_centi_percent is the process's CPU usage
//                      averaged across all cores, in 1/100ths of a percent
//                      (e.g. 1234 = 12.34%). Range 0..=10000.
//                      num_threads is the live thread count from
//                      /proc/self/stat field 20.
//   0x04 NetworkSample payload: [i64 ts_ns][u64 rx_bytes][u64 tx_bytes]   (24 bytes)
//                      rx_bytes / tx_bytes are cumulative since boot for
//                      the agent's UID, read via TrafficStats.getUidRxBytes
//                      / getUidTxBytes (host computes bps deltas).
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
constexpr uint8_t KIND_CPU = 0x03;
constexpr uint8_t KIND_NETWORK = 0x04;

// `value` carries the kind-specific u32: GC duration_us, RSS in KB, or
// cpu_centi_percent. `java_heap_kb` and `native_heap_kb` are only used for
// memory events; `num_threads` is only used for CPU events; `rx_bytes` and
// `tx_bytes` are only used for network events.
struct Event {
    uint8_t kind;
    int64_t ts_ns;
    uint32_t value;
    uint32_t java_heap_kb;
    uint32_t native_heap_kb;
    uint32_t num_threads;
    uint64_t rx_bytes;
    uint64_t tx_bytes;
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
    Event e{KIND_GC, end, (uint32_t)((end - start) / 1000), 0, 0, 0, 0, 0};
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

// Snapshot of /proc/self/stat: jiffies = utime+stime (field 14+15) and
// num_threads (field 20). Both come from a single read of the same file.
struct CpuStat {
    int64_t jiffies;
    uint32_t num_threads;
};

// Read /proc/self/stat and return jiffies (utime+stime, fields 14+15) and
// num_threads (field 20). Format is:
//   "pid (comm) state ppid pgrp session tty_nr tpgid flags
//    minflt cminflt majflt cmajflt utime stime cutime cstime
//    priority nice num_threads ..."
// `comm` can contain spaces and parentheses, so we anchor on the LAST ')' and
// skip 12 spaces past it to land on utime. Returns {0, 0} on any parse error.
CpuStat read_cpu_stat() {
    CpuStat out{0, 0};
    int fd = ::open("/proc/self/stat", O_RDONLY | O_CLOEXEC);
    if (fd < 0) return out;
    char buf[1024];
    ssize_t n = ::read(fd, buf, sizeof(buf) - 1);
    ::close(fd);
    if (n <= 0) return out;
    buf[n] = '\0';
    char* p = strrchr(buf, ')');
    if (p == nullptr) return out;
    ++p;
    int spaces = 0;
    while (*p && spaces < 12) {
        if (*p == ' ') ++spaces;
        ++p;
    }
    if (spaces < 12) return out;
    char* end = nullptr;
    long long utime = strtoll(p, &end, 10);
    if (end == p) return out;
    p = end;
    long long stime = strtoll(p, &end, 10);
    if (end == p) return out;
    p = end;
    // Skip cutime, cstime, priority, nice (fields 16..19) — strtoll auto-skips
    // leading whitespace, so four sequential calls walk us forward.
    for (int i = 0; i < 4; ++i) {
        (void)strtoll(p, &end, 10);
        if (end == p) return out;
        p = end;
    }
    long long num_threads = strtoll(p, &end, 10);
    if (end == p) return out;
    out.jiffies = (int64_t)(utime + stime);
    out.num_threads = num_threads > 0 ? (uint32_t)num_threads : 0;
    return out;
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

// JNI handles for android.net.TrafficStats per-uid byte counters. Initialized
// from the same daemon thread as `JniHeap` (reuses its JNIEnv*). On any setup
// failure or if TrafficStats reports UNSUPPORTED for our uid, `init_jni_net`
// returns false and the sampler skips emitting network events.
struct JniNet {
    jclass traffic_cls;
    jmethodID rx_mid;
    jmethodID tx_mid;
    jint uid;
};

bool init_jni_net(JNIEnv* env, JniNet* n) {
    *n = JniNet{};
    jclass local_traffic = env->FindClass("android/net/TrafficStats");
    if (local_traffic == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("FindClass for TrafficStats failed");
        return false;
    }
    n->traffic_cls = (jclass)env->NewGlobalRef(local_traffic);
    env->DeleteLocalRef(local_traffic);
    n->rx_mid = env->GetStaticMethodID(n->traffic_cls, "getUidRxBytes", "(I)J");
    n->tx_mid = env->GetStaticMethodID(n->traffic_cls, "getUidTxBytes", "(I)J");
    if (n->rx_mid == nullptr || n->tx_mid == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("GetStaticMethodID for TrafficStats getUid{Rx,Tx}Bytes failed");
        return false;
    }

    jclass proc_cls = env->FindClass("android/os/Process");
    if (proc_cls == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        LOGE("FindClass for Process failed");
        return false;
    }
    jmethodID my_uid_mid = env->GetStaticMethodID(proc_cls, "myUid", "()I");
    if (my_uid_mid == nullptr) {
        if (env->ExceptionCheck()) env->ExceptionClear();
        env->DeleteLocalRef(proc_cls);
        LOGE("GetStaticMethodID for Process.myUid failed");
        return false;
    }
    n->uid = env->CallStaticIntMethod(proc_cls, my_uid_mid);
    env->DeleteLocalRef(proc_cls);
    if (env->ExceptionCheck()) {
        env->ExceptionClear();
        LOGE("Process.myUid() threw");
        return false;
    }

    // TrafficStats.UNSUPPORTED == -1: kernel doesn't track per-uid stats on
    // this device. Probe once and disable network emission rather than send
    // a stream of zeros that look like real flat traffic.
    jlong probe = env->CallStaticLongMethod(n->traffic_cls, n->rx_mid, n->uid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); probe = -1; }
    if (probe == -1) {
        LOGI("TrafficStats reports UNSUPPORTED for uid %d; skipping network samples", n->uid);
        return false;
    }
    return true;
}

// Read cumulative rx/tx bytes for our uid via cached JNI handles. On any
// exception, treat as 0 (host treats it as a skipped tick).
void read_jni_net(JNIEnv* env, JniNet* n, uint64_t* rx, uint64_t* tx) {
    jlong rxv = env->CallStaticLongMethod(n->traffic_cls, n->rx_mid, n->uid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); rxv = 0; }
    jlong txv = env->CallStaticLongMethod(n->traffic_cls, n->tx_mid, n->uid);
    if (env->ExceptionCheck()) { env->ExceptionClear(); txv = 0; }
    *rx = rxv > 0 ? (uint64_t)rxv : 0;
    *tx = txv > 0 ? (uint64_t)txv : 0;
}

void* sampler_loop(void* arg) {
    JavaVM* vm = (JavaVM*)arg;
    JniHeap heap;
    if (!init_jni_heap(vm, &heap)) {
        LOGE("sampler stopping: JNI heap setup failed");
        return nullptr;
    }
    JniNet net;
    bool net_ok = init_jni_net(heap.env, &net);
    long clock_ticks = sysconf(_SC_CLK_TCK);
    if (clock_ticks <= 0) clock_ticks = 100;
    long num_cores = sysconf(_SC_NPROCESSORS_ONLN);
    if (num_cores < 1) num_cores = 1;
    int64_t prev_ts_ns = 0;
    int64_t prev_jiffies = 0;
    for (;;) {
        int64_t ts_ns = now_ns();
        CpuStat stat = read_cpu_stat();

        Event mem;
        mem.kind = KIND_MEMORY;
        mem.ts_ns = ts_ns;
        mem.value = read_rss_kb();
        read_jni_heap(&heap, &mem.java_heap_kb, &mem.native_heap_kb);
        mem.num_threads = 0;
        mem.rx_bytes = 0;
        mem.tx_bytes = 0;
        enqueue(mem);

        // CPU% = (delta_jiffies / clock_ticks_per_sec) / delta_secs / cores * 100.
        // Skip the first tick (no delta yet); also skip if jiffies decreased
        // (shouldn't happen, but be defensive).
        if (prev_ts_ns != 0 && stat.jiffies >= prev_jiffies) {
            int64_t delta_jiffies = stat.jiffies - prev_jiffies;
            int64_t delta_ns = ts_ns - prev_ts_ns;
            if (delta_ns > 0) {
                int64_t numer = delta_jiffies * 10000LL * 1000000000LL;
                int64_t denom = (int64_t)clock_ticks * delta_ns * (int64_t)num_cores;
                int64_t centi = (denom > 0) ? (numer / denom) : 0;
                if (centi < 0) centi = 0;
                if (centi > 10000) centi = 10000;
                Event cpu;
                cpu.kind = KIND_CPU;
                cpu.ts_ns = ts_ns;
                cpu.value = (uint32_t)centi;
                cpu.java_heap_kb = 0;
                cpu.native_heap_kb = 0;
                cpu.num_threads = stat.num_threads;
                cpu.rx_bytes = 0;
                cpu.tx_bytes = 0;
                enqueue(cpu);
            }
        }
        prev_ts_ns = ts_ns;
        prev_jiffies = stat.jiffies;

        if (net_ok) {
            Event netev;
            netev.kind = KIND_NETWORK;
            netev.ts_ns = ts_ns;
            netev.value = 0;
            netev.java_heap_kb = 0;
            netev.native_heap_kb = 0;
            netev.num_threads = 0;
            read_jni_net(heap.env, &net, &netev.rx_bytes, &netev.tx_bytes);
            enqueue(netev);
        }

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
    // GC:      [u8 kind][u32 len=12][i64 ts_ns][u32 duration_us]
    // CPU:     [u8 kind][u32 len=16][i64 ts_ns][u32 cpu_centi_percent]
    //                   [u32 num_threads]
    // Memory:  [u8 kind][u32 len=20][i64 ts_ns][u32 rss_kb]
    //                   [u32 java_heap_kb][u32 native_heap_kb]
    // Network: [u8 kind][u32 len=24][i64 ts_ns][u64 rx_bytes][u64 tx_bytes]
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
    if (e.kind == KIND_CPU) {
        uint8_t buf[1 + 4 + 16];
        buf[0] = e.kind;
        put_be32(buf + 1, 16);
        put_be64(buf + 5, e.ts_ns);
        put_be32(buf + 13, e.value);
        put_be32(buf + 17, e.num_threads);
        return send_all(fd, buf, sizeof(buf));
    }
    if (e.kind == KIND_NETWORK) {
        // put_be64 takes int64_t but only writes the bit pattern; casting u64
        // through int64_t round-trips the bytes. Host decodes with u64::from_be_bytes.
        uint8_t buf[1 + 4 + 24];
        buf[0] = e.kind;
        put_be32(buf + 1, 24);
        put_be64(buf + 5, e.ts_ns);
        put_be64(buf + 13, (int64_t)e.rx_bytes);
        put_be64(buf + 21, (int64_t)e.tx_bytes);
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
