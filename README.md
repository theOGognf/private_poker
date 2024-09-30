![TUI][0]

<div align="center">
    <i>A poker library, server, client, and TUI.</i>
</div>

# 🃟 pri♦ate_p♡ker 🃏︎

- Wanting to play poker but only have a computer and no playing cards?
- Having a slow day at work and in need of something to pass the time
  with your coworkers?
- Managing an entirely legal gambling ring and in need of a secure,
  private, and easy-to-use solution for running poker games?

If you answered "yes" to any of these rhetorical questions, then this project
is for you! Host and manage a poker game from the comfort of your computer
with **p**ri♦ate_**p**♡ker (or **pp** for short)!

# Poker over `ssh`

One can host a server within a Docker container for the following benefits:

- The server is ephemeral and more isolated from the host system
- Client binaries don't need to be distributed to users
- Users are managed by the container's user space
- Server connections are managed by `ssh`

Host and manage poker over `ssh` with the following commands:

1. Run the container (two options):

   - From source:
         
     ```bash
     docker build -t poker .
     docker run -d --name poker -p $port:22 --rm poker
     ```

   - From [the official Docker image][1]:

     ```bash
     docker run -d --name poker -p $port:22 --rm ognf/poker:latest
     ```

2. Create a user:

   ```bash
   docker exec poker ./create_user $username
   ```

   This creates a user in the container's user space and adds the user
   to a group that enables the user to send their own public `ssh` key
   to the server.

3. Users can then create an `ssh` key pair and copy their public key to
   the server:

   ```bash
   ssh-keygen -q -t rsa -b 4096 -N '' -f ~/.ssh/poker_id_rsa
   export PUBLIC_KEY="$(cat ~/.ssh/poker_id_rsa.pub)"
   ssh -i ~/.ssh/poker_id_rsa -o SendEnv=PUBLIC_KEY -p $port $username@$host
   ```

   If congratulated with a `Success` message, subsequent `ssh` commands
   will result in the user being greeted by the poker TUI. Users also no
   longer have to specify the `SendEnv` option in subsequent `ssh` commands:

   ```bash
   ssh -i ~/.ssh/poker_id_rsa -p $port $username@$host
   ```

4. Delete a user:

   ```bash
   docker exec poker ./delete_user $username
   ```

5. Stop the server:

   ```bash
   docker stop poker
   ```

# Poker without Docker

The poker over `ssh` Docker image is < 40MB, but requires some additional
user management on the host's part. If you're playing a poker game in a
local or private network, and all your users are familiar with `cargo`,
it's less work to just use the poker binaries directly rather than using
Docker and `ssh`.

## From source

- For the host, run the server binary:
   
  ```bash
  RUST_LOG=info cargo run --bin pp_server -r -- --bind $host
  ```

- For users, run the client binary:

  ```bash
  cargo run --bin pp_client -r -- $username --connect $host
  ```

## From crates.io

- For the host, install and run the server:

  ```bash
  cargo install pp_server
  RUST_LOG=info pp_server --bind $host
  ```

- For users, install and run the client:

  ```bash
  cargo install pp_client
  pp_client $username --connect $host
  ```

# Project structure

See each subdirectory's docs or `README.md`s for more specific info.

```bash
.
├── pp_admin        # Scripts and configs for managing the server within Docker
├── pp_client       # Client binary source
├── pp_server       # Server binary source
└── private_poker   # Library that the client and server use
```

# Non-goals

I use this project to learn Rust and to play poker with friends
and family. I'm probably disinterested in anything related to this
project that doesn't contribute to those goals. Specifically, the
following features are ommitted from this project and left as an
exercise to forkers:

- Server orchestration or scaling
- Persistent storage or backups of game data
- UIs beyond the TUI

# Acknowledgements

- [@Ilikemath642][2] for inspiring me to work on a poker game
- [@zachstruck][3] for teaching me a lot about Rust
- [@Mac-Genius][4] for TUI feedback
- [@shazow][5] for inspiring me with [`ssh-chat`][6]

[0]: assets/tui.png?raw=true
[1]: https://hub.docker.com/r/ognf/poker
[2]: https://github.com/Ilikemath642
[3]: https://github.com/zachstruck
[4]: https://github.com/Mac-Genius
[5]: https://github.com/shazow
[6]: https://github.com/shazow/ssh-chat
[7]: https://github.com/theOGognf/private_poker
[8]: 
