cargo b -r
git -C index fetch https://github.com/rust-lang/crates.io-index-archive.git snapshot-2024-11-27:snapshot-2024-11-27

days=1

while [ $days -lt 70 ]
do
    commit=$(git -C index log --before "$days days ago" -n 1 --all --pretty=format:"%H")
    
    echo "Processing for $days days ago... $commit"
    
    ./target/release/benchmark_from_crates --commit $commit --with-solana -t 10
    days=$((days + 4))
done