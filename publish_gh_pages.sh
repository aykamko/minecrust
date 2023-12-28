#!/bin/bash
set -eu -o pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

if [[ -n $(git status --porcelain) ]]; then
  echo "Aborting, you have uncommited git changes!" 2>&1
  exit 1
fi

if [[ $(git rev-parse --abbrev-ref HEAD) != main ]]; then
  echo "Please check out to main branch first." 2>&1
  exit 1
fi

yarn build
git branch -D github_pages # delete local github_pages branch, we will force-overwrite it
git checkout -b github_pages
yarn build
git add -f dist
git commit -m "add dist dir"
git push -uf origin HEAD

git checkout -
