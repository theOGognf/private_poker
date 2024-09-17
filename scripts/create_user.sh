username=$1
docker exec -it poker sh ./bin/create_user.sh $username
docker cp poker:/home/$username/.ssh/id_rsa ./keys/$username
