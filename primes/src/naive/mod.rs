#[derive(Debug)]
pub struct Primes {
    max: u64,
    next: u64,
    seen: Vec<u64>,
}

// By boxing it up and returning a trait object, we can use it anywhere an iterator of u64's is
// needed, so that all of our different implementations can have compatible types.
pub fn primes(max: u64) -> Box<Iterator<Item = u64>> {
    let iter = Primes {
        max,
        next: 1,
        seen: Vec::new(),
    };
    Box::new(iter)
}

impl Iterator for Primes {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        'outer: for i in self.next..=self.max {
            for j in &self.seen {
                if i % j == 0 {
                    continue 'outer;
                }
            }
            if i != 1 {
                self.seen.push(i);
                self.next = i + 1;
                return Some(i);
            }
        }
        None
    }
}
