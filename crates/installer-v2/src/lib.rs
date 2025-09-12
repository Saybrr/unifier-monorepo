

pub fn download_wabbajack(file_path: &str) -> () {

    println!("Downloading Wabbajack file  {}", file_path);
    // Implementation goes here
    ()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = download_wabbajack("Baseline/modlist");
        assert_eq!(result, ());
    }
}
