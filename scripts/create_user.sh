# Create a user in the container's user space and copy their
# private SSH key to the host's `./keys` directory.
username=$1
docker exec -it poker sh ./bin/create_user.sh $username
mkdir keys
docker cp poker:/home/$username/.ssh/id_rsa ./keys/$username
