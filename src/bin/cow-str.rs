use std::borrow::Cow;
use std::io;
use std::io::Write;

fn print_table<T: AsRef<str>>(table: Vec<Vec<T>>) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for row in table {
        for col in row {
            stdout.write_all(col.as_ref().as_bytes())?;
            stdout.write(b" ")?;
        }
        stdout.write(b"\n")?;
    }
    Ok(())
}

fn main() {
    let mut table = Vec::new();
    let dash = "-";

    for i in 0..10 {
        let mut row: Vec<Cow<str>> = Vec::new();
        row.push("hello".into());
        row.push("world".into());
        row.push(dash.into());
        row.push(format!("{}", i).into());

        for item in &row {
            match item {
                Cow::Borrowed(x) => eprintln!("borrowed: {}", x),
                Cow::Owned(x) => eprintln!("owned: {}", x),
            }
        }

        table.push(row);
    }
    print_table(table).expect("oy");
}
