#!/usr/bin/env bash

script_dir="$(dirname "$0")"
base_url="https://dl-cdn.alpinelinux.org/alpine/"
dir="${script_dir}/repositories"
versions_file="$dir/versions.txt"

mkdir -p "${dir}"

curl -o "${dir}/index.html" "${base_url}"

grep -oP '(?<=href=")[^"]*(?=/")' "${dir}/index.html" | grep -E '^v[0-9]+\.[0-9]' | sort -V | awk -F'[v.]' '{ if ($2 > 3 || ($2 == 3 && $3 > 11)) print $2"."$3 }' > "${versions_file}"

rm "${dir}/index.html"

while read -r version; do
    version_dir=""
    apkindex_url="${base_url}v${version}/main/x86_64/APKINDEX.tar.gz"
    curl -o "${dir}/${version}-APKINDEX.tar.gz" "${apkindex_url}"
    mkdir -p "${dir}/${version}"
    tar -xzf "${dir}/${version}-APKINDEX.tar.gz" -C "${dir}/${version}"
    rm "${dir}/${version}-APKINDEX.tar.gz"
done < "${versions_file}"
