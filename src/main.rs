use std::env;

fn main() {
    let project = env::var("PROJECT").expect("Project is not provided");
    let token = env::var("TOKEN").expect("Token is not provided");
    let out_file = env::var("OUT_FILE").expect("Out file is not provided");

    turnslate::run(&project, &token, &out_file)
}
