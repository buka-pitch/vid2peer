#!/usr/bin/env bash
# Cross-compile the Rust networking core for Android (arm64-v8a, armeabi-v7a,
# x86_64) and copy the shared libraries into the Android app's jniLibs.
#
# Requirements:
#   - Rust targets installed:
#       rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   - Android NDK installed; export ANDROID_NDK_HOME (or ANDROID_NDK_ROOT)
#
# Usage:
#   ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/26.x.x ./android/build-ffi.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$ROOT/target"
OUT_ABI_DIR="$ROOT/android/app/src/main/jniLibs"

if [[ -z "${ANDROID_NDK_HOME:-}" && -n "${ANDROID_NDK_ROOT:-}" ]]; then
  ANDROID_NDK_HOME="$ANDROID_NDK_ROOT"
fi

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  echo "error: set ANDROID_NDK_HOME (or ANDROID_NDK_ROOT) to your NDK path" >&2
  exit 1
fi

# Map cargo target -> Android ABI -> jniLibs folder
declare -A TARGETS=(
  [aarch64-linux-android]=arm64-v8a
  [armv7-linux-androideabi]=armeabi-v7a
  [x86_64-linux-android]=x86_64
)

for target in "${!TARGETS[@]}"; do
  abi="${TARGETS[$target]}"
  echo ">> building $target ($abi)"
  rustup target add "$target"
  cargo build -p p2p-video-chat-android-ffi --release --target "$target"

  mkdir -p "$OUT_ABI_DIR/$abi"
  cp "$TARGET_DIR/$target/release/libp2pvc_ffi.so" "$OUT_ABI_DIR/$abi/"
  echo "   copied libp2pvc_ffi.so -> $OUT_ABI_DIR/$abi/"
done

echo "done. jniLibs tree:"
find "$OUT_ABI_DIR" -type f
