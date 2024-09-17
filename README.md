<div align="center">
    <i>A poker library, server, client, and TUI</i>
</div>

![TUI][0]

# 🃟 pri♦ate_p♡ker 🃏︎

- Do you want to play poker, and you have a computer but no deck of cards?
- Is it a slow day at work and you're looking to kill some time with coworkers?
- Are you running a family friendly and perfectly legal poker ring and you want
  a reasonably secure, private, and out-of-the-box solution for running and
  managing said ring?

If you answered "yes" to any of these rhetorical questions, then this project
is for you! Host and manage a poker game from the comfort of your computer with
🃟 **p**ri♦ate_**p**♡ker 🃏︎ (**pp** for short)!

# Poker over `ssh`

One can host the server using the provided `Dockerfile` for the following
benefits:

- The server is ephemeral and more isolated from the host system
- Client binaries don't need to be distributed to users
- Server connections are entirely managed with `ssh`
- Users are entirely managed by the OS and a couple of utility scripts

Host and manage poker over `ssh` with the following commands:

1. Build the image
   
   ```bash
   docker build . -t poker
   ```

2. Run the container

   ```bash
   # Make sure you name it "poker"
   docker run --name poker -p $port:22 --rm poker
   ```

3. Create a user

   ```bash
   # This creates the user in the OS and copies
   # their private key to the host `./keys` directory.
   # Send the user their key so they can SSH into the
   # server and play.
   ./scripts/create_user.sh $username
   ```

4. Users can SSH into the server and play

   ```bash
   ssh -i $poker_ssh_key -p $port $username@$host
   ```

5. Delete a user

   ```bash
   ./scripts/delete_user.sh $username
   ```

6. Clean-up the server entirely

   ```bash
   docker stop poker
   ```

# Organization

See each subdirectory's `README.md` for more specific info.

```bash
├── pp_admin        # Scripts and configs for managing the server within Docker
├── pp_client       # Client binary source
├── pp_server       # Server binary source
├── private_poker   # Library that the client and server use
└── scripts         # Scripts for managing the server outside Docker
```

# Non-goals

I use this project to learn Rust and play poker with friends
and family. I'm probably disinterested in anything related to this
project that doesn't fit those goals. Specifically, the following
are ommitted from **pp** and left as an exercise to forkers:

- Integrating or scaling servers with orchestration layers
- Persistent storage and/or backups of game data
- UIs besides the already-implemented TUI

# Acknowledgements

- [@Ilikemath642][1] for inspiration for poker
- [@zachstruck][2] for teaching me a lot about Rust
- [@Mac-Genius][3] for TUI feedback
- [@shazow][4] for inspiration from [`ssh-chat`][5]

[0]: https://github.com/theOGognf/private_poker/blob/39b586751eae28033b6c1e086b81bfbd6ce74729/assets/tui.png?raw=true
[1]: https://github.com/Ilikemath642
[2]: https://github.com/zachstruck
[3]: https://github.com/Mac-Genius
[4]: https://github.com/shazow
[5]: https://github.com/shazow/ssh-chat
