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

-------------
Register:
curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/register


Login:
curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/login



create repos:
curl -X POST -H "Authorization: Bearer your_long_auth_token_string" http://localhost:3000/repos/my-new-repo


delete repos:
curl -X DELETE -H "Authorization: Bearer your_long_auth_token_string" http://localhost:3000/repos/my-new-repo
