#!/bin/sh

# Let a user write their own public key to their authorized_keys
# file.
ulimit -f 1048576
if [ -z "$PUBLIC_KEY" ]; then
    echo "Error: PUBLIC_KEY not set"
    exit 1
fi
echo $PUBLIC_KEY > $HOME/.ssh/authorized_keys \
&& echo "Success"
