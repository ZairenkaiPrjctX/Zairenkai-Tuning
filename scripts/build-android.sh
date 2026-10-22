#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright 2026 ZairenkaiPrjctX contributors
# Build ztune for Android arm64 as a component of the unified zairenkai module, together
# with its Corresponding Source (GPL-3.0 section 6).
#
# Requires: Rust with the aarch64-linux-android target, ANDROID_NDK_HOME pointing at an
# Android NDK (CI uses r27d), git, tar, gzip, and network access for `cargo vendor` (or a
# populated cargo cache). The working tree must be clean: the source archive is made from
# HEAD, and the binary is built from exactly that tree.
#
# Output: dist/zairenkai-tuning/
#   bin/ztune                                  the executable
#   LICENSE NOTICE THIRD_PARTY.md SOURCE.md     licence, notices, how to get and build the source
#   source/zairenkai-tuning-<commit>-src.tar.gz the Corresponding Source, dependencies vendored
#   component.json                              what Zairenkai-Modules checks before packaging
set -euo pipefail

cd "$(dirname "$0")/.."
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to an Android NDK directory}"
API=29
LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android${API}-clang"
[ -x "$LINKER" ] || { echo "linker not found: $LINKER" >&2; exit 1; }
if [ -n "$(git status --porcelain)" ]; then
  echo "the working tree is not clean; commit first so the source archive matches the binary" >&2
  exit 1
fi

COMMIT=$(git rev-parse HEAD)
EPOCH=$(git log -1 --format=%ct HEAD)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
OUT=dist/zairenkai-tuning
SRC_NAME="zairenkai-tuning-$COMMIT-src"

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER"
export ZTUNE_COMMIT="$COMMIT"
cargo build --locked --release --target aarch64-linux-android -p ztune
BIN=target/aarch64-linux-android/release/ztune
# 16 KB page-size devices need 16 KB-aligned LOAD segments (.cargo/config.toml sets the flag).
for align in $("$(dirname "$LINKER")/llvm-readelf" -lW "$BIN" | awk '$1 == "LOAD" { print $NF }'); do
  [ $((align)) -ge 16384 ] || { echo "ztune: LOAD segment aligned to $align, need 16 KB" >&2; exit 1; }
done

rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/source"
cp "$BIN" "$OUT/bin/ztune"
chmod 0755 "$OUT/bin/ztune"
cp LICENSE NOTICE "$OUT/"
cp docs/THIRD_PARTY.md "$OUT/THIRD_PARTY.md"

# Corresponding Source: the tree at HEAD plus every crate it links, vendored, so the binary
# can be rebuilt offline from the archive alone (see SOURCE.md inside it).
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
git archive --format=tar --prefix="$SRC_NAME/" HEAD | tar -x -C "$STAGE"
(cd "$STAGE/$SRC_NAME" && cargo vendor --locked --versioned-dirs vendor >vendor-config.toml)
{
  echo
  echo "# Added by scripts/build-android.sh: build from the vendored crates in this archive."
  cat "$STAGE/$SRC_NAME/vendor-config.toml"
} >>"$STAGE/$SRC_NAME/.cargo/config.toml"
rm "$STAGE/$SRC_NAME/vendor-config.toml"
tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner \
  -C "$STAGE" -cf - "$SRC_NAME" | gzip -n -9 >"$OUT/source/$SRC_NAME.tar.gz"

cat >"$OUT/SOURCE.md" <<MD
# Source of ztune (GPL-3.0-only)

This directory's \`bin/ztune\` was built from ZairenkaiPrjctX/Zairenkai-Tuning at commit
\`$COMMIT\` (version $VERSION) with \`scripts/build-android.sh\`.

The complete Corresponding Source is \`source/$SRC_NAME.tar.gz\`, shipped next to the binary:
the repository tree at that commit plus every Rust crate the binary links, vendored. To rebuild:

    tar -xzf source/$SRC_NAME.tar.gz && cd $SRC_NAME
    ANDROID_NDK_HOME=<ndk r27d> ZTUNE_COMMIT=$COMMIT \\
      CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=\$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android$API-clang \\
      cargo build --offline --locked --release --target aarch64-linux-android -p ztune

Licence: GNU General Public License version 3 only (\`LICENSE\`). Upstream projects and the
files adapted from them: \`NOTICE\` and \`docs/PROVENANCE.md\` in the archive.
MD

files=""
for f in bin/ztune LICENSE NOTICE THIRD_PARTY.md SOURCE.md "source/$SRC_NAME.tar.gz"; do
  files="$files${files:+,}
    \"$f\": \"$(sha256sum "$OUT/$f" | cut -d' ' -f1)\""
done
cat >"$OUT/component.json" <<JSON
{
  "format": 1,
  "component": "zairenkai-tuning",
  "repository": "ZairenkaiPrjctX/Zairenkai-Tuning",
  "version": "$VERSION",
  "commit": "$COMMIT",
  "target": "aarch64-linux-android",
  "min_api": $API,
  "license": "GPL-3.0-only",
  "source": "source/$SRC_NAME.tar.gz",
  "files": {$files
  }
}
JSON
cat "$OUT/component.json"
