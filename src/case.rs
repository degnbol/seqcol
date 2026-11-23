use std::collections::{HashMap, HashSet};

/// Uppercase of a character.
///
/// # Arguments
/// * `c` - Character
///
/// # Returns
/// The uppercase of `c` if it is one character, else `c`. For example, `ß` (uppercase `SS`) stays `ß`.
pub fn upper(c: char) -> char {
    single(c.to_uppercase()).unwrap_or(c)
}

/// Lowercase of a character.
///
/// # Arguments
/// * `c` - Character
///
/// # Returns
/// The lowercase of `c` if it is one character, else `c`.
pub fn lower(c: char) -> char {
    single(c.to_lowercase()).unwrap_or(c)
}

/// The item of an iterator with exactly one item, else `None`.
fn single<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    items.next().filter(|_| items.next().is_none())
}

/// Characters in both cases.
///
/// # Arguments
/// * `chars` - Characters
///
/// # Returns
/// `chars`, and the [`upper`] and [`lower`] of each.
pub fn both_cases(chars: &HashSet<char>) -> HashSet<char> {
    chars.iter().flat_map(|&c| [c, upper(c), lower(c)]).collect()
}

/// Add the other case of each key, with the value of that key.
///
/// # Arguments
/// * `map` - Value of each character. Values of the keys in `map` do not change.
///   A new key gets the value of its [`upper`] if that is a key, else of its [`lower`].
/// * `own` - Characters that do not become keys, e.g. as they have a value of their own in another map
pub fn add_other_case<V: Copy>(map: &mut HashMap<char, V>, own: &HashSet<char>) {
    let other_cases: Vec<(char, V)> = both_cases(&map.keys().copied().collect())
        .into_iter()
        .filter(|c| !map.contains_key(c) && !own.contains(c))
        .filter_map(|c| map.get(&upper(c)).or(map.get(&lower(c))).map(|&value| (c, value)))
        .collect();
    map.extend(other_cases);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_cases_adds_single_character_cases() {
        assert_eq!(both_cases(&HashSet::from(['A', '-'])), HashSet::from(['A', 'a', '-']));
        assert_eq!(both_cases(&HashSet::from(['ß'])), HashSet::from(['ß']));
    }

    #[test]
    fn add_other_case_keeps_values_of_its_own() {
        let mut map = HashMap::from([('A', 1), ('c', 2), ('G', 3), ('g', 4)]);
        add_other_case(&mut map, &HashSet::from(['C']));
        assert_eq!(map, HashMap::from([('A', 1), ('a', 1), ('c', 2), ('G', 3), ('g', 4)]));
    }
}
