clone repo:
git clone http://localhost:3000/test.git


create repo:
curl -X POST http://localhost:3000/repos/my-new-repo
git clone http://localhost:3000/my-new-repo.git


delete repo:
curl -X DELETE http://localhost:3000/repos/my-new-repo


list repos:
curl -X GET http://localhost:3000/repos

list branches:
curl -X GET http://localhost:3000/repos/test/branches

list commit:
curl -X GET http://localhost:3000/repos/test/commits/main

list file:
curl -X GET http://localhost:3000/repos/test/tree/main






------------------------
postgresql:

initialize the PostgreSQL database cluster:
initdb -D ./.data/db


star db server:
pg_ctl -D ./.data/db -l logfile start
pg_ctl -D .data/db -l logfile -o "-c listen_addresses='*'" start
pg_ctl -D .data/db -l logfile -o "-c listen_addresses='*' -c unix_socket_directories='/tmp'" start


create the database:
createdb -h 127.0.0.1 githome


drop db:
dropdb -h 127.0.0.1 git_clone_db

migrate table with sqlx:
cargo install sqlx-cli
sqlx migrate add -r create_repositories_table

/home/user/.cargo/bin/sqlx migrate run

nixos:
export PATH="/home/user/.cargo/bin:$PATH" && sqlx migrate run

-------------
Register:
curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/register


Login:
curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/login



create repos:
curl -X POST -H "Authorization: Bearer your_long_auth_token_string" http://localhost:3000/repos/my-new-repo

curl -X POST -H "Authorization: Bearer ZtA4MIHLofk7R7uMoCGTiaPllrPlU6BK" -H "Content-Type: application/json" -d '{"name": "my-new-repo", "public": false}' http://localhost:3000/repos


delete repos:
curl -X DELETE -H "Authorization: Bearer your_long_auth_token_string" http://localhost:3000/repos/my-new-repo



create test repo:
curl -X POST -H "Authorization: Bearer 4t1boMGWbCZFhxcq46ClJi9VJT0BBJFI" -H "Content-Type: application/json" -d '{"name": "test-repo", "public": false}' http://localhost:3000/repositories

pr:
curl -X POST -H "Authorization: Bearer 4t1boMGWbCZFhxcq46ClJi9VJT0BBJFI" -H "Content-Type: application/json" -d '{"title": "Test PR", "body": "This is a test PR", "base_branch": "main", "head_branch": "feature-branch"}' http://localhost:3000/repos/test-repo/pulls


curl -X POST -H "Authorization: Bearer AvxE3rmPCWenMSKdusOZyovhLqpZEYcI" -H "Content-Type: application/json" -d '{"title": "Test PR", "body": "Test PR description", "head_branch": "feature-branch", "base_branch": "main"}' http://localhost:3000/repos/test-repo/pulls

diff:
curl -H "Authorization: Bearer AvxE3rmPCWenMSKdusOZyovhLqpZEYcI" http://localhost:3000/repos/test-repo/pulls/4/diff

review:
curl -X POST -H "Authorization: Bearer AvxE3rmPCWenMSKdusOZyovhLqpZEYcI" -H "Content-Type: application/json" -d '{"body": "This is a test review.", "status": "approved"}' http://localhost:3000/repos/test-repo/pulls/4/reviews

comments:
curl -X POST -H "Authorization: Bearer AvxE3rmPCWenMSKdusOZyovhLqpZEYcI" -H "Content-Type: application/json" -d '{"body": "This is a test comment."}' http://localhost:3000/repos/test-repo/pulls/4/comments

testmergeing:
curl -X PATCH -H "Authorization: Bearer AvxE3rmPCWenMSKdusOZyovhLqpZEYcI" -H "Content-Type: application/json" -d '{"status": "merged"}' http://localhost:3000/repos/test-repo/pulls/4

remove branch:
git --git-dir=repos/test-repo.git branch -d feature-branch


merge:
curl -X POST -H "Content-Type: application/json" -d '{"username": "merge_tester", "password": "password"}' http://localhost:3000/register
curl -X POST -H "Content-Type: application/json" -d '{"username": "merge_tester", "password": "password"}' http://localhost:3000/login

curl -X POST -H "Content-Type: application/json" -H "Authorization: Bearer sKR9I00esR9ZU9rrldPN3OkZlRCGyGpo" -d '{"name": "merge-test"}' http://localhost:3000/repos
git clone ./repos/merge-test.git

need main branch
cd merge-test && git checkout -b main && echo "# Merge Test" > main.md && git add main.md && git commit -m "Initial commit on main" && git push origin main

cd merge-test && git checkout -b feature-branch && echo "hello world" > README.md && git add README.md && git commit -m "Add README"
cd merge-test && git push origin feature-branch

curl -X POST -H "Content-Type: application/json" -H "Authorization: Bearer sKR9I00esR9ZU9rrldPN3OkZlRCGyGpo" -d '{"title": "Test PR", "body": "This is a test pull request", "base_branch": "main", "head_branch": "feature-branch"}' http://localhost:3000/repos/merge-test/pulls


merging pr:
curl -X PATCH -H "Content-Type: application/json" -H "Authorization: Bearer sKR9I00esR9ZU9rrldPN3OkZlRCGyGpo" -d '{"status": "merged"}' http://localhost:3000/repos/merge-test/pulls/3

cd merge-test && git pull origin main && ls






