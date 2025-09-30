#!/bin/sh
set -e

# Create a user in the Alpine Docker container, make their home directory, and
# listen for when they update their authorized_keys file so we can finalize
# their poker account.
username="$1"
ssh_dir="/home/$username/.ssh"

adduser -D "$username"
addgroup "$username" unclaimed
passwd -d "$username"
mkdir -p "$ssh_dir"
touch "$ssh_dir/authorized_keys"
chmod 644 "$ssh_dir/authorized_keys"
chown "$username":"$username" -R "$ssh_dir"
nohup sh -c "
    inotifywait -e close_write '/home/$username/.ssh/authorized_keys' \
    && delgroup '$username' unclaimed
" > "./$username.log" 2>&1 < /dev/null &
