set working-directory := "crates"
set positional-arguments

rust_min_stack := "8388608" # 8 MiB

help:
    just -l

# `a365dt`
a365dt *args:
    cargo run --bin a365dt -- {{ args }}

fmt:
    cargo fmt --all

fix *args:
    cargo clippy --fix --tests --allow-dirty {{ args }}

clippy *args:
    cargo clippy --tests {{ args }}

crate name *args:
    cargo new {{ name }} {{ args }}

[unix]
install:
    rustup show active-toolchain
    cargo fetch

[unix]
test *args:
    RUST_MIN_STACK={{ rust_min_stack }} cargo test --no-fail-fast {{ args }}

bench *args:
    cargo bench --workspace --bench '*' {{ args }}

bench-smoke:
    just bench -- --test

build-release:
    cargo build --release

clean:
    cargo clean
