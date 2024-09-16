username=$1
ssh_pub_key=$2

adduser -D $username
passwd -d $username
mkdir -p "/home/$username/.ssh"
chmod 700 /home/$username/.ssh
touch "/home/$username/.ssh/authorized_keys"
chmod 644 "/home/$username/.ssh/authorized_keys"
chown $username:$username -R "/home/$username"
echo "$ssh_pub_key" > "/home/$username/.ssh/authorized_keys" 
