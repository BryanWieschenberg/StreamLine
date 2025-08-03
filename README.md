# StreamLine

A powerful, customizable, and performant chat platform built in Rust. Supports hundreds of users across multiple rooms via LAN, features end-to-end encrypted messaging, a modular command-driven interface, and a deeply configurable role-based access control system that puts user control at the forefront.

---

## Core Features

- Real-time messaging over LAN with multiple concurrent clients across multiple rooms
- Vast command system for user/room management, moderation, etc. Stored locally as JSON files
- Rooms with custom role permissions on a per-command basis
- End-to-end encryption and password hashing using SHA-256

---

## Install & Setup
###### This program has only been tested to work on Ubuntu and Windows, though very likely works on all major operating systems. Instructions have been provided only for Ubuntu and Windows.

#### Run these commands on every device that will run the chat program:

Ensure you have installed these dependencies:
- Rust: [Download](https://rustup.rs/#)
- Git: [Download](https://git-scm.com/downloads)

Once those are installed, run these commands in your terminal:
```bash
git clone https://github.com/BryanWieschenberg/StreamLine.git
cd StreamLine
```

#### Server setup:
In the machine you want to have to server run on:
- Ubuntu: ```ip addr```
- Windows: ```ipconfig```

Take note of your server machine's IP address. This is what client machines on your LAN will use to connect to the server.

You also will need to open a port for incoming connections on your server's machine.
- Ubuntu: ```sudo ufw allow <port>```
- Windows: Run the server binary normally, and if you get prompted to by the Firewall to open the port for incoming connections, click Accept

Run the server binary:
```
cargo run --bin server [-q <port>]
```

#### Client setup:
To join via LAN, clients must specify the server machine's IP address and port.

Once the server is running, clients may join the server by running the client binary:
```
cargo run --bin client [-q <ip_address:port>]
```

---

## Commands

Most commands are dependent on the client's current state (e.g., guest, logged in, in-room) and access level. In each room, there are 4 permission levels: Owner, Admin, Moderator, and User. The Moderator and User roles can have many commands can be added to or revoked from their usage, while Owners and Admins retain the ability to use all commands. There is only 1 Owner per room, and the Owner of a room is fully protected from being assigned a lower role, and is the only one allowed to delete their own room. Many commands rely on the data present in local storage, located in `/data/users.json` and `/data/rooms.json`.

Many commands have shorter, more concise variations for more experienced users. To see all the variations, check how the program parses commands in `/src/commands/parser.rs`.

#### Universal Commands (Always available)
- `/help` - Shows available commands
- `/clear` - Clears the chat window
- `/quit` - Exits the program
- `/ping` - Displays round-trip latency in milliseconds

#### Lobby Commands

#### `/account`
- `register <username> <password> <confirm_password>` - Registers a new user, hashes their password, generates their private/public keys for end-to-end encryption on the clientside, and shares the user data, hashed password, and public key, with the server
- `login <username> <password>` - Logs in with existing credentials and informs the server of the user's public key
- `logout` - Logs out current user and reverts them to a guest
- `edit username <new_username>` - Changes your username. Only unique usernames are allowed
- `edit password <new_password> <confirm_new_password>` - Changes your password. Remains hashed
- `import <file_name>` - Imports account data from JSON files in `/data/vault/users`
- `export [<file_name>]` - Exports your account data as a JSON file into `/data/logs/users`. The [\<file_name>] option allows users to name the exported file
- `delete [force]` - Deletes your account. The [force] option allows users to skip the deletion prompt

#### `/room` (Must be logged in)
- `list` - Lists available rooms (only public rooms or ones you're whitelisted in)
- `join <room_name>` - Joins the specified room if the user has access to it
- `create <room_name> [<whitelist>]` - Creates a new room and sets you as the owner. The [whitelist] option allows the room to be private upon creation
- `import <file_name>` - Imports a room from JSON files in `data/vault/rooms` (Export variant is mentioned later since it requires you to be in the room and have superuser privileges)
- `delete [force] <room_name>` - Deletes the specified room (Owner only). The [force] option allows users to skip the deletion prompt

#### `/ignore` (Must be logged in, and works in and out of rooms)
- `list` - Shows who you're currently ignoring (users you block messages from)
- `add <user1> <user2> ...` - Adds users to the runner's ignore list
- `remove <user1> <user2> ...` - Removes users from the runner's ignore list

#### In-Room Commands
- `/leave` - Leaves your current room and sends you back to the lobby
- `/status` - Displays information about you in your current room
- `/afk` - Marks you as AFK until you type again
- `/msg <username>` - Sends a private message to the specified user
- `/me <message>` - Third-person message (e.g., *\* Bryan waves*)
- `/seen <user>` - Shows when the specified user was last online in the room
- `/announce <message>` - Message sent to the entire room (bypasses ignores of the sender)

#### `/super` (Superuser Tools)
- `users` - Shows all online user data in that room (including hidden, banned, muted, etc.). A higher-privilege version of /user list
- `rename <new_name>` - Edits the room name. Only unique room names are allowed
- `export [<file_name>]` - Expxorts your current room data as a JSON file into `/data/vault/rooms`. The [\<file_name>] option allows users to name the exported file
- `whitelist`
  - `info` - Shows the current whitelist state
  - `toggle` - Toggles whitelist on or off for the current room
  - `add <user1> <user2> ...` - Adds users to the room whitelist
  - `remove <user1> <user2> ...` - Removes users from the room whitelist
- `limit`
  - `info` - Displays the current rate limiting/session timeout info
  - `rate <limit>|*` - Rate limiting for how many messages users can type per 5 seconds. Max value is 255. Using * fully stops rate limiting
  - `session <seconds>|*` - Controls how long a user session can go without activity before being timed out and kicked from the room. A background housekeeper thread checks every 60 seconds to see who has exceeded their room's threshold. Using * fully stops session timeouts
- `roles`
  - `list` - Shows the current command permissions for Users and Moderators (Admins and Owners are always granted all permissions)
  - `add <user|mod> <command1> <command2> ...` - Grants addable/revokable commands to the specified role (Addable/revokable commands are listed later)
  - `revoke <user|mod> <command1> <command2> ...` - Revokes addable/revokable commands from the specified role
  - `assign <user|mod|admin|owner> <user1> <user2> ...` - Assigns the specified role to the user. Only current Owners can assign users as Owner, and assigning another user as Owner transfers Ownership exclusively to that user
  - `recolor <user|mod|admin|owner> <hex_color>` - Sets the color for the specified role's prefix

#### `/user` (User Customization)
- `list` - Lists visible users in the room
- `rename <nickname>` - Sets your nickname in this room
- `recolor <hex_color>` - Changes your name color in this room
- `hide` - Hides you from this room's /user list. Does not hide you from /super users

#### `/mod` (Moderation Utilities)
- `kick <username> [<reason>]` - Kicks user from room. The [\<reason>] option shows the kicked user the reason why upon being kicked
- `ban <username> [<days>d<hrs>h<mins>m<secs>s|*] [<reason>]` - Bans user. By default, the ban time is permanent, but the banner can specify the length with the [\<days>d\<hrs>h\<mins>m\<secs>s|*] option. For example, 3d12h bans a user for 3 days 12 hours. The ban length can be written in any time, so something like 30s1h10m is acceptible. Using * bans the user permanently, so if you want to ban the user permanently and provide a [\<reason>] option, use that
- `unban <username>` - Unbans specified user
- `mute <username> [<days>d<hrs>h<mins>m<secs>s|*] [<reason>]` - Mutes user (same arguments as ban)
- `unmute <username>` - Unmutes specified user

#### Addable/Revokeable Commands
Many commands can be added to or revoked from the User/Moderator roles. These codes can be used as arguments in the `/super roles add` or `/super roles revoke` commands. Some of these are parent codes, meaning if a role has the parent code, they can access all child commands. If they have child codes, they can only access those specific child commands:
- `afk`
- `msg`
- `me`
- `seen`
- `announce`
- `super` **(Parent)**
  - `super.users`
  - `super.rename`
  - `super.export`
  - `super.whitelist`
  - `super.limit`
  - `super.roles`
- `user` **(Parent)**
  - `user.list`
  - `user.rename`
  - `user.recolor`
  - `user.hide`
- `mod` **(Parent)**
  - `mod.info`
  - `mod.kick`
  - `mod.ban`
  - `mod.mute`

Default Mod Commands:
- `afk`
- `msg`
- `me`
- `seen`
- `super.users`
- `user`
- `mod`

Default User Commands:
- `afk`
- `msg`
- `me`
- `seen`
- `user`

## Security

A detailed list of security tests can be found in `security.txt`. 28/37 tests passed, so approximately 76% of unauthorized actions were reduced with the command and role-based access control system.

If you would like a deeper look into the security of this program, please check the text file listed above. Since it is a LAN-based application, not over the whole Internet, it is expected to only be used by trusted individuals. While many security precautions were taken, some vulnerabilities do still remain.

Some remaining vulnerabilities include:
- Server data not being protected from individuals who gain access to the server's machine
- Command-based messages not be end-to-end encrypted
- Brute-forcing logins, due to no rate limiting in the lobby
- No input validation for Unicode/emoji characters in usernames/rooms, which clients cannot properly decrypt messages from

## Screenshots

![First img](https://imgur.com/a/sKxKfEn)
