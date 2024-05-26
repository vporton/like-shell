#!/bin/sh

# Copy all hooks from .githooks to .git/hooks
echo "Setting up Git hooks..."
cp -r .githooks/* .git/hooks/
echo "Git hooks set up successfully."
