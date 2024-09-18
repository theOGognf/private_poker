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

One can host a server with the provided `Dockerfile` for the following
benefits:

- The server is ephemeral and more isolated from the host system
- Client binaries don't need to be distributed to users
- Server connections are entirely managed with `ssh`
- Users are entirely managed by the container's user space and a couple
  of utility scripts

Host and manage poker over `ssh` with the following commands:

1. Build the image:
   
   ```bash
   docker build . -t poker
   ```

2. Run the container:

   ```bash
   docker run --name poker -p $port:22 --rm poker
   ```

   Make sure the container's name is `"poker"` as the utils in `./scripts`
   make that assumption.

3. Create a user:

   ```bash
   ./scripts/create_user.sh $username
   ```

   This creates a user in the container's user space and copies
   their private key to the host's `./keys` directory. Send the
   user their key so they can SSH into the server and start playing.

4. Users can SSH into the server and play:

   ```bash
   ssh -i $poker_ssh_key -p $port $username@$host
   ```

5. Delete a user:

   ```bash
   docker exec -it poker deluser --remove-home $username
   ```

6. Stop the server:

   ```bash
   docker stop poker
   ```

# Organization

See each subdirectory's `README.md` for more specific info.

```bash
.
├── pp_admin        # Scripts and configs for managing the server within Docker
├── pp_client       # Client binary source
├── pp_server       # Server binary source
├── private_poker   # Library that the client and server use
└── scripts         # Scripts for managing the server outside Docker
```

# Non-goals

I use this project to learn Rust and to play poker with friends
and family. I'm probably disinterested in anything related to this
project that doesn't fit those goals. Specifically, the following
features are ommitted from this project and left as an exercise to
forkers:

- Server orchestration or scaling
- Persistent storage or backups of game data
- UIs beyond the TUI

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
