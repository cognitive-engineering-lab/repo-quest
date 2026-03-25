mod operations;

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    use crate::operations::*;

    #[test]
    fn test_add() {
        assert_eq!(add(1,1), 2);
    }
}
