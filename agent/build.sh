#!/usr/bin/env bash
# Cross-compiles libholoagent.so for arm64-v8a and x86_64 using the Android NDK.
# Output is committed to agent/prebuilt/<abi>/ and embedded into Holo.
#
# Run on a dev machine when the agent changes; CI does not run this.

set -euo pipefail

NDK="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/29.0.13113456}"
API=28

cd "$(dirname "$0")"

if [ ! -d "$NDK" ]; then
  echo "NDK not found at $NDK — set ANDROID_NDK_HOME" >&2
  exit 1
fi

case "$(uname -s)" in
  Darwin) HOST_TAG=darwin-x86_64 ;;
  Linux)  HOST_TAG=linux-x86_64 ;;
  *) echo "unsupported host: $(uname -s)" >&2; exit 1 ;;
esac

TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/$HOST_TAG"

build() {
  local abi="$1"
  local triple="$2"
  local clang="$TOOLCHAIN/bin/${triple}${API}-clang++"
  local out="prebuilt/$abi/libholoagent.so"
  local debug="prebuilt/$abi/libholoagent.debug.so"

  mkdir -p "prebuilt/$abi"
  # The agent uses only POSIX (pthread, sockets) — no libstdc++/libc++ — so
  # there's nothing to link statically. -fno-exceptions / -fno-rtti keep
  # us from emitting EH tables that would interact badly with ART's
  # unwinder when our callbacks run on its GC thread.
  "$clang" \
    -std=c++17 -fPIC -shared -O2 -g \
    -Wall -Wextra \
    -fno-exceptions -fno-rtti \
    -nostdlib++ \
    -fvisibility=hidden \
    -Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384 \
    -Wl,-z,separate-loadable-segments \
    -Wl,-z,now -Wl,-z,relro \
    -Wl,--exclude-libs,ALL \
    -I. \
    -o "$out" \
    agent.cpp \
    -llog
  cp "$out" "$debug"
  "$TOOLCHAIN/bin/llvm-strip" "$out"
  echo "built $out ($(wc -c < "$out") bytes), debug copy at $debug"
}

build arm64-v8a aarch64-linux-android
build x86_64    x86_64-linux-android
