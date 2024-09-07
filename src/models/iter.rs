use core::panic;
use std::{
    iter::{Cycle, Peekable, Take},
    vec::IntoIter,
};

#[derive(Debug)]
pub struct CyclicIterator<'a, T: Clone> {
    cyclic: Cycle<IntoIter<T>>,
    n: usize,
    iter: Peekable<Take<&'a mut Cycle<IntoIter<T>>>>,
}

impl<'a, T: Clone> CyclicIterator<'a, T> {
    pub fn new(items: Vec<T>) -> Self {
        if items.is_empty() {
            panic!("The idea is to have at least one item to loop around")
        }

        let mut cyclic = items.into_iter().cycle();

        CyclicIterator {
            n: items.len(),
            iter: cyclic.by_ref().take(items.len()).peekable(),
            cyclic,
        }
    }

    pub fn reset(&mut self) -> T {
        self.iter = self.cyclic.by_ref().take(self.n).peekable();
        self.iter
            .peek()
            .expect("Should contain at least one item")
            .clone()
    }

    pub fn peek(&self) -> Option<&T> {
        self.iter.peek()
    }
}

impl<'a, T: Clone> Iterator for CyclicIterator<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_iteration() {
        let vec = vec![0, 1, 2, 3, 4, 5];
        let mut cyclic = CyclicIterator::new(vec.clone());

        let result: Vec<_> = cyclic.iter.collect();
        assert_eq!(result, vec);

        cyclic.reset();

        let result: Vec<_> = cyclic.iter.collect();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 0]);
    }

    #[test]
    fn test_single_element() {
        let vec = vec!['A'];
        let mut cyclic = CyclicIterator::new(vec);

        let take = cyclic.iter;

        assert_eq!(take.next(), Some('A'));
        assert_eq!(take.next(), None);
    }

    #[test]
    fn test_string_iteration() {
        let vec = vec!["apple", "banana", "cherry"];
        let mut cyclic = CyclicIterator::new(vec.clone());

        let result: Vec<_> = cyclic.iter.collect();
        assert_eq!(result, vec);

        let result: Vec<_> = cyclic.iter.collect();
        assert_eq!(result, vec!["banana", "cherry", "apple"]);
    }
}
