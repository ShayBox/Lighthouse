all: windows linux

windows:
	cargo build --release --target=i686-pc-windows-gnu

linux:
	cargo build --release --target=x86_64-unknown-linux-gnu
	strip target/x86_64-unknown-linux-gnu/release/lighthouse
