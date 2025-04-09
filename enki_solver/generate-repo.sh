find "$1" -type f -name 'opam' | xargs -I {} sh -c 'opam2json {} > {}.json'
