use chrono::Local;

pub fn log(message: &str) {
    println!("[{}] {}",
             Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
             message);
}