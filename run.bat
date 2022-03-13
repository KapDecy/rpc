set NAME=rpc
cargo build
copy target\debug\%NAME%.exe %NAME%.exe
rpc.exe