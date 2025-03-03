#!/usr/bin/env bash

script_dir="$(dirname "$0")"
base_url="http://ftp.debian.org/debian/dists/"
dir="${script_dir}/repositories"
versions_file="$dir/versions.txt"

mkdir -p "${dir}"

# Define the versions you want to download
echo -e "buster\nbullseye\nbookworm" > "${versions_file}"

while read -r version; do
    version_dir="${dir}/${version}"
    mkdir -p "${version_dir}"
    # TODO other archs
    packages_url="${base_url}${version}/main/binary-amd64/Packages.gz"
    curl -o "${version_dir}/Packages.gz" "${packages_url}"
    gunzip "${version_dir}/Packages.gz"
done < "${versions_file}"
