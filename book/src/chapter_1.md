# Programming a Guessing Game

Our first project will be to implement a guessing game from scratch. The game should work like this:
- The program will greet the player.
- It will generate a random integer between 1 and 100. 
- It will then prompt the player to enter a guess. 
- After a guess is entered, the program will indicate whether the guess is too low or too high. 
- If the guess is correct, the game will print a congratulatory message and exit.
- Otherwise, the game keeps going.

## Setting Up a New Project

First, setup a new project:

```console
$ cargo new guessing_game
$ cd guessing_game
```

<details>
  <summary>What does this do?</summary>
  
  The first command, `cargo new`, takes the name of the project (`guessing_game`) as the first argument. The second command changes to the new project’s directory.   
</details>

Then run the project:

```console
$ cargo run
```

It should print `Hello world!`.

<details>
  <summary>Why does it print this?</summary>
  
  This is the default behavior for new binaries. Look at `src/main.rs`.
</details>

## Processing a Guess

Open `src/main.rs` and look at the `main` function. Right now, it contains the one line:

```rust
println!("Hello, world!");
```

<task> 
<span>Task 1:</span> greet the player when they run the binary by printing exactly this text:

```console
$ cargo run
Guess the number!
Please input your guess.
```
</task>

For this first project, we will check your work with a provided testing binary called `guessing-game-grader`. Before making your changes, try running it to observe the failing test:

```console
$ guessing-game-grader
✗ Task 1: Prints the right initial strings
  The diff is:
     H̶e̶l̶l̶o̶,̶ ̶w̶o̶r̶l̶d̶!̶
     Guess the number!
     Please input your guess.
```

Once you make the correct edit, the grader should show the test as passing and move on to the next test, like this:

```console
$ guessing-game-grader
✓ Task 1: Prints the right initial strings
✗ Task 2: Accepts an input
  [...]
```

<details>
  <summary>I need a hint!</summary>
  
  TODO
</details>

<details>
  <summary>I need the solution!</summary>
  
  TODO
</details>

Once you're ready, move on to the next task.

<task> 
<span>Task 2:</span> read a string from the command line and print it out, like this:

```console
$ cargo run
Guess the number!
Please input your guess.
15
You guessed: 15
```
</task>

The key method is [`Stdin::read_line`]. A core skill in the Rust It Yourself series  will be learning to read documentation of unfamiliar methods, so let's read the documentation for `read_line`:

<details>
  <summary>How was I supposed to know that <code>Stdin::read_line</code> is the key method?</summary>

  TODO (Google, StackOverflow, ChatGPT, top-down search of the docs, ...)
</details>

> ```rust
> pub fn read_line(&self, buf: &mut String) -> Result<usize>
> ```
> Locks this handle and reads a line of input, appending it to the specified buffer.
> For detailed semantics of this method, see the documentation on [`BufRead::read_line`].
> ##### Examples
> ```rust
> use std::io;
> 
> let mut input = String::new();
> match io::stdin().read_line(&mut input) {
>     Ok(n) => {
>         println!("{n} bytes read");
>         println!("{input}");
>     }
>     Err(error) => println!("error: {error}"),
> }
> ```

The first line is the **type signature** of `read_line`. It says: "I am a method that takes two arguments: an immutable reference to myself ([`Stdin`]), and a mutable reference to a [`String`]. I return a type called `Result<usize>`."

This type signature and example use Rust features we haven't discussed yet. That's expected &mdash; another core skill in the Rust It Yourself series is working with code that uses features you don't fully understand. So let's try and learn something from this example anyway.

The `read_line` method demonstrates two key aspects of Rust: 

1. **Mutability:** Rust requires you to be more explicit about when data is mutated in-place. Here, calling `read_line` mutates `input` in-place. That is represented by the fact that the second argument is not a plain `String`, but instead a mutable reference `&mut String`. Additionally, the variable `input` must be declared as `mut` so we are allowed to mutate it. 

2. **Error handling:** Rust does not have a concept of `undefined` (as in JS) or `None` (as in Python) or `NULL` (as in C++) or `nil` (as in Go). Rust also does not have exceptions. Instead, to represent "this operation can succeed and return `X` or fail and return `Y`", Rust uses enums in combination with pattern matching via operators like `match`. Unlike enums in other languages, Rust's enums can have fields (similar to tagged unions in C).

Now, try copy-pasting this example into the bottom of your `main` function. Run the code (with `cargo run`), see how it works, and try editing the example so it completes Task 2.



[`std::io::stdin`]: https://doc.rust-lang.org/std/io/fn.stdin.html
[`Stdin`]: https://doc.rust-lang.org/std/io/struct.Stdin.html
[`Stdin::read_line`]: https://doc.rust-lang.org/std/io/struct.Stdin.html#method.read_line
[`BufRead::read_line`]: https://doc.rust-lang.org/std/io/trait.BufRead.html#method.read_line
[`String`]: https://doc.rust-lang.org/std/string/struct.String.html