# Create a user in the Alpine Docker container,
# make their home directory, create an SSH key pair,
# and put their public key within their authorized keys
# file.
username=$1
ssh_dir=/home/$username/.ssh

adduser -D $username \
&& passwd -d $username \
&& mkdir -p $ssh_dir \
&& touch $ssh_dir/authorized_keys \
&& chmod 644 $ssh_dir/authorized_keys \
&& chown $username:$username -R $ssh_dir \
&& ssh-keygen -q -t rsa -b 4096 -N '' -f $ssh_dir/id_rsa \
&& mv $ssh_dir/id_rsa.pub $ssh_dir/authorized_keys
