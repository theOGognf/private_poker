#!/bin/sh

# Delete a user in the Alpine Docker container and close
# their SSH connection so they can't continue to make
# actions in the game.
username=$1

pkill -U $username
deluser --remove-home $username
