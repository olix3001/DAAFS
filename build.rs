fn main() {
    let dotenv_path = dotenv::dotenv().expect("Failed to load .env file");
    println!("cargo:rerun-if-changed={}", dotenv_path.display());

    for env_var in dotenv::dotenv_iter().unwrap() {
        let (key, value) = env_var.unwrap();
        println!("cargo:rustc-env={}={}", key, value);
    }
}