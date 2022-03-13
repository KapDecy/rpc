set NAME=rpc
echo "start build"
cargo build
copy target\debug\%NAME%.exe %NAME%.exe
echo "done"