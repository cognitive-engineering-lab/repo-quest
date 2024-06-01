use std::{cmp::Ordering, io};

use rand::Rng;

fn main() {
  println!("Guess the number!");
  println!("Please input your guess.");

  let secret_number = rand::thread_rng().gen_range(1..=100);

  loop {
    let mut guess = String::new();
    io::stdin()
      .read_line(&mut guess)
      .expect("Failed to read line");

    let Ok(guess) = guess.trim().parse::<u32>() else {
      println!("Please type a number!");
      continue;
    };

    println!("You guessed: {guess}");

    match guess.cmp(&secret_number) {
      Ordering::Less => println!("Too small!"),
      Ordering::Greater => println!("Too big!"),
      Ordering::Equal => {
        println!("You win!");
        break;
      }
    }
  }
}
