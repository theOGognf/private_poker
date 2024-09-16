ssh-keygen -t rsa -b 4096
cat /root/.ssh/id_rsa.pub >> /home/poker/.ssh/authorized_keys
ssh -i /root/.ssh/id_rsa aws@localhost
