use core::panic;

#[derive(Debug)]
pub struct CyclicIterator<T: Clone> {
    items: Vec<T>,
    current_index: usize,

    iteration_count: usize,
}

impl<T: Clone> CyclicIterator<T> {
    pub fn new(items: Vec<T>) -> Self {
        if items.is_empty() {
            panic!("The idea is to have at least one item to loop around")
        }

        CyclicIterator {
            items,
            current_index: 0,
            iteration_count: 0,
        }
    }

    pub fn reset(&mut self) -> T {
        self.iteration_count = 0;
        self.current_index += 1;
        self.items[self.current_index].clone()
    }

    pub fn peek(&self) -> Option<&T> {
        if self.iteration_count < self.items.len() {
            self.items.get(self.current_index)
        } else {
            None
        }
    }
}

impl<T: Clone> Iterator for CyclicIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iteration_count < self.items.len() {
            let item = self.items[self.current_index].clone();
            self.current_index = (self.current_index + 1) % self.items.len();
            self.iteration_count += 1;
            Some(item)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_iteration() {
        let vec = vec![0, 1, 2, 3, 4, 5];
        let mut cyclic = CyclicIterator::new(vec.clone());

        let result: Vec<_> = cyclic.by_ref().collect();
        assert_eq!(result, vec);

        cyclic.reset();

        let result: Vec<_> = cyclic.collect();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 0]);
    }

    #[test]
    fn test_single_element() {
        let vec = vec!['A'];
        let mut cyclic = CyclicIterator::new(vec);

        assert_eq!(cyclic.next(), Some('A'));
        assert_eq!(cyclic.next(), None);
    }

    #[test]
    fn test_string_iteration() {
        let vec = vec!["apple", "banana", "cherry"];
        let mut cyclic = CyclicIterator::new(vec.clone());

        let result: Vec<_> = cyclic.by_ref().collect();
        assert_eq!(result, vec);

        let result: Vec<_> = cyclic.by_ref().collect();
        let vec: Vec<&str> = vec![];
        assert_eq!(vec, result);

        cyclic.reset();

        let result: Vec<_> = cyclic.collect();
        assert_eq!(result, vec!["banana", "cherry", "apple"]);
    }
}
