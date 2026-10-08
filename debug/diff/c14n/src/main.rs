use minify_html::canonicalise;
use std::io::Read;
use std::io::stdin;
use std::io::stdout;

fn main() {
    let mut src = Vec::new();
    stdin().read_to_end(&mut src).unwrap();
    canonicalise(&mut stdout(), &src).unwrap();
}
