# Build project
cargo build

# Run nbdkit
nbdkit \
    --foreground \
    --verbose \
    --exit-with-parent \
    --filter=blocksize \
    ./target/debug/libdaafs.so \
    minblock=4K \
    maxdata=4K