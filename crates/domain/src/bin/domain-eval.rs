use std::io::{self, BufRead};

fn main() -> io::Result<()> {
    for line in io::stdin().lock().lines() {
        println!("{}", campuslingo_domain::evaluate_json(&line?));
    }
    Ok(())
}
