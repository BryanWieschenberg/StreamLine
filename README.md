# StreamLine

A secure, multithreaded terminal-based chatroom built in Rust. It supports real-time messaging, multiple rooms, end-to-end encryption, and a powerful command-based interface with vast customizability for role-based permissions.

## Features

- Real-time messaging with multiple concurrent clients
- Vast CLI command system allowing for high levels of customization
- Account system with secure password handling using `bcrypt`
- Individual rooms with custom role permissions on a per-command basis
- Whitelist-based access control with toggleable settings
- Users and rooms are stored locally as `.json` files, which can be easily imported/exported
- Fully asynchronous runtime using `tokio` multithreadding, and shared state management using `Arc<Mutex<_>>`
- End-to-end encryption using symmetric AES via `rustls`
- Optional rate limiting and session timeouts to prevent spam/flooding
- Built-in accurate round-trip latency calculation via `/ping`
- Powerful custom interface with via `TUI` featuring multiple windows for a smoother UI/UX
- Supports file sharing via chunked Base64 as a secure delivery to targeted recipients within the room

## Install

```bash
git clone https://github.com/BryanWieschenberg/StreamLine.git
cd StreamLine
cargo build --release
```

## Command Documentation

Most commands are organized by context (e.g., not signed in, lobby, in-room) and access level. Some commands can be added to or revoked from the moderator or user roles. Admins retain all commands, and the owner of a room is fully protected from being assigned a lower role. Many commands rely on the data present in local storage, located in `/data/users.json` and `/data/rooms.json`.

### Universal Commands (Always available)
- `/help` - Shows available commands
- `/clear` - Clears the chat window
- `/quit` - Exits the program
- `/ping` - Displays round-trip latency in milliseconds

---

### Lobby Commands (Only available when not in a room)

#### `/account`
- `register <username> <password> <confirm_password>` - Registers a new user
- `login <username> <password>` - Logs in with existing credentials
- `logout` - Logs out current user
- `edit username <new_username>` - Changes your username
- `edit password <new_password> <confirm_new_password>` - Changes your password
- `import <file_name>` - Imports account data from JSON
- `export` - Exports your account data as a timestamped JSON file in `/data/logs/users`
- `export <file_name>` - Exports your account data to the given file name
- `delete` - Deletes your account after your confirm with a prompt
- `delete force` - Deletes your account without prompting for confirmation
#### `/room` (Only after logging in)
- Lists available rooms (only public rooms or ones you're whitelisted in)
- `join <room_name>` - Joins the specified room
- `create <room_name>` - Creates a new room and sets you as the owner
- `delete <room_name>` - Deletes the specified room (owner only)
- `import <file_name>` - Imports a room from JSON

---

### In-Room Commands

- `/leave` - Leaves your current room
- `/status` - Displays your username, role, and color in this room
- `/afk` - Marks you as AFK until you type again
- `/uptime` - Shows server uptime
- `/sendfile <file_name> <recipient>` - Sends a base64 file over TCP
- `/msg <username>` - Sends a private message in-room
- `/me <message>` - Third-person message (e.g., *Bryan waves*)

---

### /super Commands (Admin/Mod Only)
- `users` - Lists all users in the room with roles, colors, status
- `reset` - Resets room config to default (removes whitelist, resets perms)
- `rename <new_name>` - Renames the current room
- `export` - Exports the room config as a timestamped JSON file
- `export <file_name>` - Exports to a specific file_name
- `whitelist` - Shows whitelist
  - `toggle` - Toggles whitelist on/off
  - `add <user1> <user2> ...` - Adds user(s) to whitelist
  - `delete <user1> <user2> ...` - Removes user(s) from whitelist
- `roles` - Shows role-permission mappings
  - `add <role> <command1> <command2>` - Adds permissions to role
  - `revoke <role> <command1> <command2>` - Removes permissions
  - `assign <username> <user|mod|admin|owner>` - Sets user role
  - `recolor <role> <hex_color>` - Sets default color for role
    - `force` - Enforces role color, overrides user colors

---

### /user Commands
- `list` - Lists visible users in the room
- `rename <nickname>` - Sets your nickname in this room
- `recolor <hex_color>` - Changes your name color in this room
- `hide` - Hides you from `/room` user lists
- `ignore` - Lists who you're ignoring
  - `<username>` - Toggles ignoring that user
  - `all` - Toggles ignoring that user in all rooms

---

### /log Commands (Client-side only)
- `list` - Lists saved logs in `/log`
- `save` - Saves current chat to timestamped file
- `save <file_name>` - Saves chat with custom name
- `load` - Loads most recent log
- `load <file_name>` - Loads specified log (only works if room still exists)

---

### /mod Commands (Mods/Admins Only)
- `/kick <username>` - Kicks user from room
- `/ban <username>` - Bans user (default: forever)
  - `|<days|d|hours|h|minutes|m|seconds|s>` - Custom ban duration
- `/unban <username>` - Removes user from ban list
- `/mute <username>` - Mutes user (default: forever)
  - `|<days|d|hours|h|minutes|m|seconds|s>` - Custom mute duration
- `/unmute <username>` - Unmutes user

---

### Addable/Revokeable Commands
While admins can access every command, many commands can be added to or revoked from the mod/user roles:
- `afk`
- `uptime`
- `sendfile`
- `msg`
- `me`
- `super`
  - `super.users`
  - `super.reset`
  - `super.rename`
  - `super.export`
  - `super.whitelist`
  - `super.limit`
  - `super.roles`
- `user`
  - `user.list`
  - `user.rename`
  - `user.recolor`
  - `user.ignore`
  - `user.hide`
- `log`
  - `log.list`
  - `log.save`
  - `log.view`
- `mod`
  - `mod.kick`
  - `mod.ban`
  - `mod.mute`

Default Mod Commands:
- `afk`
- `uptime`
- `sendfile`
- `msg`
- `me`
- `super.users`
- `user`
- `log`
- `mod`

Default User Commands:
- `afk`
- `uptime`
- `sendfile`
- `msg`
- `me`
- `user`
- `log`