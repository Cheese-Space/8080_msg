# 8080_msg: a self-hostable chat server written in rust  
⚠️ WARNING: 8080_msg is currently in beta, so don’t expect the best or most stable experience.  
There is currently also ***++NO ENCRYPTION++*** of messages so use at your own risk.  
  
Are you tired of Big Tech™ constantly leaking your valuable information and do you want to exactly know how your messages are processed and received?  
And do you also just want to have a simple messaging app for yourself (and your friends)?  
Well then the 8080_msg project is just for you!  
  
## features
1. self-hostable chat server
2. cross-platform terminal client
3. ability to make custom clients (not recomended for now)
4. save chat history in a sqlite database
5. everything in the 8080_msg project is and will always be free and open-source
  
## project overview  
The 8080_msg project consists of 3 parts:  
  
1. **lib8080_msg**  
Lib8080_msg is the internal library used by the server and client of the 8080_msg project.  
It defines the core types and methods used.  
  
2. **server_8080**  
Server_8080 is the server used by the 8080_msg project.  
It does everything you would expect a server to do like sending and receiving data, listening for connections etc.  
  
3. **tty_8080**  
And last but not least tty_8080: the official terminal client of the 8080_msg project.  
The interface is kinda ugly for now, but will hopefully be updated in the future.  

## roadmap for v1.0.0 (new things may be added in the future)
1. [x] working prototype of all the components
2. [x] database for storing message history
3. [ ] file transfers 
4. [ ] encryption with tls
5. [ ] 'upgrade' the look of tty_8080
6. [ ] some form of authentication
7. [ ] have multiple rooms in one server
8. [ ] add software for managing the server
9. [ ] add presistent user profile in tty_8080
10. [ ] create a guide for making a custom client 
11. [ ] (optional) create a simple gui client
12. [ ] (optional) create a simple web client
  
## the protocol  
more info about the custom network protocol used coming (soon)!  
  
## building a custom client  
coming (soon)!  

## building from source
Building from soure is very easy.  
First clone and cd into the repo:
``` bash
git clone https://github.com/Cheese-Space/8080_msg
cd 8080_msg
```
Then cd into the part you want to build, for example tty_8080:
``` bash
cd tty_8080
```
And finally build/run via cargo:
``` bash
cargo run -- --adress localhost --port 8080 --username cheese_space
```
