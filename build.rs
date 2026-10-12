fn main() {
    // Регистрируем cfg-условие, которое использует web-sys для unstable API.
    // Без этой строки rustc выдаёт warning:
    //   unexpected `cfg` condition name: `web_sys_unstable_apis`
    println!("cargo::rustc-check-cfg=cfg(web_sys_unstable_apis)");
}