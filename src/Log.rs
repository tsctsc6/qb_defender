pub fn log(message: &str) {
    println!("[{}] {}",
             chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
             message);
}