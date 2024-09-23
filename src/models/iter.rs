#[derive(Debug)]
pub struct CyclicIterator {
    items_count: usize,
    current_index: usize,
    iteration_count: usize,
}

impl CyclicIterator {
    pub fn new(count: usize) -> Self {
        CyclicIterator {
            items_count: count,
            current_index: 0,
            iteration_count: 0,
        }
    }

    pub fn advance(&mut self) -> usize {
        self.iteration_count = 0;
        let current = self.current_index;
        self.current_index = (self.current_index + 1) % self.items_count;
        current
    }

    pub fn reset(&mut self) -> usize {
        self.iteration_count = 0;
        self.current_index
    }

    pub fn reset_on(&mut self, idx: usize) -> usize {
        self.current_index = idx;
        self.reset()
    }

    pub fn peek(&self) -> Option<usize> {
        if self.iteration_count < self.items_count {
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn peek_next(&self) -> Option<usize> {
        if self.iteration_count + 1 < self.items_count {
            Some((self.current_index + 1) % self.items_count)
        } else {
            None
        }
    }
}

impl Iterator for CyclicIterator {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iteration_count < self.items_count {
            let current = Some(self.current_index);

            self.iteration_count += 1;
            self.current_index = (self.current_index + 1) % self.items_count;

            current
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
        let mut cyclic = CyclicIterator::new(5);

        let result: Vec<_> = cyclic.by_ref().collect();
        assert_eq!(result, vec![0, 1, 2, 3, 4]);

        cyclic.advance();
        let result: Vec<_> = cyclic.by_ref().collect();
        assert_eq!(result, vec![1, 2, 3, 4, 0]);

        cyclic.advance();
        let result: Vec<_> = cyclic.collect();
        assert_eq!(result, vec![2, 3, 4, 0, 1]);
    }

    #[test]
    fn test_single_element() {
        let mut cyclic = CyclicIterator::new(1);

        assert_eq!(cyclic.next(), Some(0));
        assert_eq!(cyclic.next(), None);
    }

    #[test]
    fn test_empty() {
        let mut cyclic = CyclicIterator::new(0);

        assert_eq!(cyclic.next(), None);

        cyclic.reset();

        assert_eq!(cyclic.next(), None);
    }
}
