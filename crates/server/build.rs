fn main() {
    // 画面 (frontend/dist) はバイナリに埋め込むので、ビルドし直したら再コンパイルする
    println!("cargo:rerun-if-changed=../../frontend/dist");
}
