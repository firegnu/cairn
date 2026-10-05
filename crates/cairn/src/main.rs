use clap::Parser;

fn main() {
    let cli = match cairn::cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            eprintln!("{}", one_line(&error.to_string()));
            std::process::exit(2);
        }
        Err(error) => {
            print!("{error}");
            return;
        }
    };
    match cairn::cli::run(cli, &mut std::io::stdin().lock()) {
        Ok(message) => println!("{}", one_line(&message)),
        Err(error) => {
            eprintln!("{}", one_line(&error.to_string()));
            std::process::exit(1);
        }
    }
}

fn one_line(message: &str) -> String {
    message.lines().collect::<Vec<_>>().join(" ")
}
