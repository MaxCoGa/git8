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

