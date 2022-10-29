set NAME=rpc
echo "start build"
cargo build --release
copy target\release\%NAME%.exe %NAME%.exe
echo "done"