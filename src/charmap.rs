use std::collections::HashMap;

/// Value of each character, and of all other characters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharMap<T> {
    /// Value of each character.
    pub by_char: HashMap<char, T>,
    /// Value of each character not in `by_char`, or `None` if they have no value.
    pub other: Option<T>,
}

impl<T> CharMap<T> {
    /// Value of a character.
    ///
    /// # Arguments
    /// * `c` - Character
    ///
    /// # Returns
    /// The value of `c` in `by_char`, else `other`.
    pub fn get(&self, c: char) -> Option<&T> {
        self.by_char.get(&c).or(self.other.as_ref())
    }

    /// Apply a function to each value.
    ///
    /// # Arguments
    /// * `f` - Function of a value
    ///
    /// # Returns
    /// `f` of each value of `by_char` and of `other`.
    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> CharMap<U> {
        CharMap { by_char: self.by_char.iter().map(|(&c, value)| (c, f(value))).collect(), other: self.other.as_ref().map(f) }
    }
}

impl<T: Clone> CharMap<T> {
    /// Lay `top` over `self`.
    ///
    /// # Arguments
    /// * `top` - Values that take precedence
    ///
    /// # Returns
    /// `top` if it has `other`, as `top` then has a value for all characters.
    /// Else `self` with the values of `top.by_char` added.
    pub fn overlay(mut self, top: &CharMap<T>) -> CharMap<T> {
        if top.other.is_some() {
            return top.clone();
        }
        self.by_char.extend(top.by_char.iter().map(|(&c, value)| (c, value.clone())));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_falls_back_to_other() {
        let map = CharMap { by_char: HashMap::from([('A', 1)]), other: Some(2) };
        assert_eq!((map.get('A'), map.get('C')), (Some(&1), Some(&2)));
        assert_eq!(CharMap { other: None, ..map }.get('C'), None);
    }

    #[test]
    fn overlay_with_other_replaces_all_values() {
        let base = CharMap { by_char: HashMap::from([('A', 1)]), other: Some(2) };
        let uniform = CharMap { by_char: HashMap::new(), other: Some(3) };
        assert_eq!(base.clone().overlay(&uniform), uniform);
        let top = CharMap { by_char: HashMap::from([('C', 4)]), other: None };
        assert_eq!(base.overlay(&top), CharMap { by_char: HashMap::from([('A', 1), ('C', 4)]), other: Some(2) });
    }
}
