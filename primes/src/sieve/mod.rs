#[derive(Debug)]
pub struct Primes {
    max: u64,
    next: u64,
    eliminated: Vec<bool>,
}

// By boxing it up and returning a trait object, we can use it anywhere an iterator of u64's is
// needed, so that all of our different implementations can have compatible types.
pub fn new(max: u64) -> Box<Iterator<Item = u64>> {
    let mut eliminated = vec![false; (max + 1) as usize];
    eliminated[0] = true;
    eliminated[1] = true;
    let iter = Primes {
        max: max,
        next: 1,
        eliminated: eliminated,
    };
    return Box::new(iter);
}

impl Iterator for Primes {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        for n in self.next..=self.max {
            if !self.eliminated[n as usize] {
                let mut current = n + n;
                while current <= self.max {
                    self.eliminated[current as usize] = true;
                    current = current + n;
                }
                self.next = n + 1;
                return Some(n);
            }
        }
        return None;
    }
}
