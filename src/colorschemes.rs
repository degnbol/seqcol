use std::collections::HashMap;
use yansi::Color::{self, *};

use include_dir::include_dir;

pub fn load_colorschemes() -> HashMap<String, HashMap<char, Color>> {
    let mut colorschemes = HashMap::new();
    // Read at compile time, i.e. no performance penalty at run-time for file io.
    for file in include_dir!("data/colorschemes/").files() {
        let filename = file.path().file_name().unwrap();
        let name = filename.to_str().unwrap().strip_suffix(".tsv").unwrap();

        let mut colorscheme = HashMap::new();
        let contents = file.contents_utf8().unwrap();
        for line in contents.split('\n') {
            if line != "" {
                let (c, hex) = line.split_once('\t').unwrap();
                let c = c.chars().next().unwrap(); // should only be one element long
                let r = u8::from_str_radix(hex.get(1..3).unwrap(), 16).unwrap();
                let g = u8::from_str_radix(hex.get(3..5).unwrap(), 16).unwrap();
                let b = u8::from_str_radix(hex.get(5..7).unwrap(), 16).unwrap();
                colorscheme.insert(c, Rgb(r, g, b));
            }
        }

        colorschemes.insert(name.to_string(), colorscheme);
    }
    colorschemes
}
