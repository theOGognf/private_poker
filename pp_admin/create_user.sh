#!/bin/sh

# Create a user in the Alpine Docker container,
# make their home directory, create an SSH key pair,
# and put their public key within their authorized keys
# file.
username=$1
ssh_dir=/home/$username/.ssh

adduser -D $username \
&& addgroup $username newbs \
&& passwd -d $username \
&& mkdir -p $ssh_dir \
&& touch $ssh_dir/authorized_keys \
&& chmod 644 $ssh_dir/authorized_keys \
&& chown $username:$username -R $ssh_dir \
&& nohup sh -c "inotifywait -e close_write /home/$username/.ssh/authorized_keys && delgroup $username newbs" \
    > ./$username.log 2>&1 < /dev/null &
