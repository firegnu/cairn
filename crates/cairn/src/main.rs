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
    if let cairn::cli::Command::Hook { agent } = cli.command {
        cairn::hook::execute(agent);
        return;
    }
    std::process::exit(cairn::cli::report(cairn::cli::run(
        cli,
        &mut std::io::stdin().lock(),
    )));
}

fn one_line(message: &str) -> String {
    message.lines().collect::<Vec<_>>().join(" ")
}
