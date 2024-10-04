#!/usr/bin/env bash

set -e

case "${OSTYPE}" in
  darwin*) os='Darwin' ;;
  linux*)  os='Linux' ;;
  msys*)   os='Windows' ;;
  *)       echo "unknown OSTYPE ${OSTYPE}"; exit 1 ;;
esac
arch="$(uname -m)"
ver="1.0.4"
conformance_tgz="connectconformance-v${ver}-${os}-${arch}.tar.gz"
url="https://github.com/connectrpc/conformance/releases/download/v${ver}/${conformance_tgz}"

cd "$(dirname "${BASH_SOURCE[0]}")"

conformance=".work/conformance-${ver}"

[[ -e "${conformance}" ]] || (
    echo "Downloading ${url}"
    mkdir .work || true
    cd .work
    wget -nv "${url}"
    tar xf "${conformance_tgz}"
    mv connectconformance "../${conformance}"
)

cargo build
$conformance "$@" --conf conformance.yaml --mode client -- target/debug/connect-rpc-conformance
