extern crate term;
extern crate clap;

use term::color;

use std::fs::{self, DirEntry};
use std::path::Path;
use std::io;

use std::cmp::Ordering;

use clap::{Arg, App};

mod dirsign {
    pub const HORZ: char = '─';
    pub const CROSS: char = '├';
    pub const VERT: char = '│';
    pub const LAST_FILE: char = '└';
    pub const BLANK: char = '\u{00A0}';
}

fn path_to_str(dir: &Path) -> &str {
    dir.file_name()
        .and_then(|x| x.to_str())
        .or_else(|| dir.to_str())
        .unwrap_or("")
}

fn order_dir_entry(a: &DirEntry, b: &DirEntry) -> Ordering {
    a.file_name().cmp(&b.file_name())
}

fn get_sorted_dir_entries(path: &Path) -> io::Result<Vec<DirEntry>> {
    fs::read_dir(path)
        .map(|entries| {
            let mut dir_entries : Vec<DirEntry> = entries.filter_map(Result::ok).collect();
            dir_entries.sort_by(order_dir_entry);
            dir_entries
        })
}

fn line_prefix<'a>(levels: &mut Vec<bool>, prefix_buffer: &'a mut String) -> &'a mut String {
    let len        = levels.len();
    let index      = len.saturating_sub(1);
    // factor = 4, because in each iteration pushes at least 3 chars in if/else plus one in the
    // for{} block, plus 4 in the last if{} in this function
    let mut prefix = prefix_buffer;
    // we are reusing the prefix buffer from a previous iteration, so clear it
    prefix.clear();
    for level in levels.iter().take(index) {
        if *level {
            prefix.push(dirsign::VERT);
            for _ in 0..2 {
                prefix.push(dirsign::BLANK)
            }
        } else {
            for _ in 0..3 {
                prefix.push(' ');
            }
        }

        prefix.push(' ');
    }

    if let Some(last) = levels.last() {
        if *last {
            prefix.push(dirsign::CROSS);
        } else {
            prefix.push(dirsign::LAST_FILE);
        }

        prefix.push(dirsign::HORZ);
        prefix.push(dirsign::HORZ);
        prefix.push(' ');
    }

    prefix
}

fn write_color(t: &mut Box<term::StdoutTerminal>,
               config: &Config,
               color: color::Color,
               str: &str)
               -> io::Result<()> {
    if config.use_color {
        t.fg(color)?;
    }

    write!(t, "{}", str)?;

    if config.use_color {
        t.reset()?;
    }

    Ok(())
}

fn print_path(is_dir: bool,
              file_name: &str,
              t: &mut Box<term::StdoutTerminal>,
              config: &Config,
              prefix: &str)
              -> io::Result<()> {
    write!(t, "{}", prefix)?;
    if is_dir {
        write_color(t, config, color::BRIGHT_BLUE, file_name)?;
        writeln!(t, "")
    } else {
        writeln!(t, "{}", file_name)
    }
}

fn is_hidden_file_name(file_name: &str) -> bool {
    file_name != "." && file_name != ".." && file_name.starts_with(".")
}

fn is_hidden_path(dir: &Path) -> bool {
    dir.components()
        .last()
        .and_then(|x| {
            let str = x.as_ref()
                .to_str()
                .unwrap_or("");
            Some(is_hidden_file_name(str))
        }).unwrap_or(false)
}

fn iterate_folders(path: &Path,
                   levels: &mut Vec<bool>,
                   t: &mut Box<term::StdoutTerminal>,
                   config: &Config,
                   prefix_buffer: &mut String)
                   -> io::Result<()> {
    let file_name = path_to_str(path);
    if !config.show_hidden && is_hidden_path(path) {
        return Ok(());
    }

    let is_dir = path.is_dir();
    let mut prefix_buffer = line_prefix(levels, prefix_buffer);

    if let Ok(link_path) = fs::read_link(path) {
        write!(t, "{}", &prefix_buffer)?;
        write_color(t, config, color::BRIGHT_CYAN, file_name)?;
        write!(t, " -> ")?;
        let link_path = format!("{}\n", link_path.display());
        if is_dir {
            write_color(t, config, color::BRIGHT_BLUE, &link_path)?;
        } else {
            write!(t, "{}", link_path)?;
        }

        return Ok(());
    }

    print_path(is_dir, file_name, t, config, &prefix_buffer)?;

    if levels.len() >= config.max_level {
        return Ok(());
    }

    if is_dir {
        let dir_entries = get_sorted_dir_entries(path);
        if let Err(err) = dir_entries {
            let error_msg = format!("Could not read directory '{}': {}\n", path.display(), err);
            write_color(t, config, color::RED, &error_msg)?;
            return Ok(());
        }

        let dir_entries = dir_entries.unwrap();

        levels.push(true);
        let len_entries = dir_entries.len();
        for entry in dir_entries.iter().take(len_entries.saturating_sub(1)) {
            iterate_folders(&entry.path(), levels, t, config, &mut prefix_buffer)?;
        }

        levels.pop();
        levels.push(false);
        if let Some(entry) = dir_entries.last() {
            iterate_folders(&entry.path(), levels, t, config, &mut prefix_buffer)?;
        }
        levels.pop();
    }

    Ok(())
}

struct Config {
    use_color: bool,
    show_hidden: bool,
    max_level: usize,
}

fn to_int(v: &str) -> Result<usize, String> {
    use std::str::FromStr;

    FromStr::from_str(v)
        .map_err(|e| format!("Could not parse '{}' as an integer: {}", &v, e))
}

fn main() {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("a")
            .short("a")
            .long("all")
            .help("Show hidden files"))
        .arg(Arg::with_name("color_on")
            .short("C")
            .help("Turn colorization on always"))
        .arg(Arg::with_name("color_off")
            .short("n")
            .help("Turn colorization off always"))
        .arg(Arg::with_name("DIR")
            .index(1)
            .help("Directory you want to search"))
        .arg(Arg::with_name("level")
            .short("L")
            .long("level")
            .takes_value(true)
            .validator(|s| to_int(&s).map(|_| ()))
            .help("Descend only level directories deep"))
        .get_matches();

    let use_color = matches.is_present("color_on") || !matches.is_present("color_off");

    let max_level = if let Some(level) = matches.value_of("level") {
        to_int(&level).expect("Should have validated that this value was int...")
    } else {
        usize::max_value()
    };

    let config = Config {
        use_color: use_color,
        show_hidden: matches.is_present("a"),
        max_level: max_level,
    };

    let path = Path::new(matches.value_of("DIR").unwrap_or("."));

    let mut vec: Vec<bool> = Vec::new();
    let mut t = term::stdout().unwrap();
    let mut prefix_buffer = String::with_capacity(10);
    iterate_folders(&path, &mut vec, &mut t, &config, &mut prefix_buffer).expect("Program failed");
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use super::*;

    #[test]
    fn test_is_hidden() {
        assert!(true == is_hidden_file_name(".git"));
        assert!(false == is_hidden_file_name("."));
        assert!(false == is_hidden_file_name(".."));

        assert!(false == is_hidden_path(&Path::new(".")));
        assert!(false == is_hidden_path(&Path::new("..")));
        assert!(false == is_hidden_path(&Path::new("../..")));
    }

    #[test]
    fn path_is_file_is_dir() {
        let path = Path::new(".");
        assert!(path.is_file() == false);
        assert!(path.is_dir() == true);
    }
}
