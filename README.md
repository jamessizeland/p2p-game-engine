# Peer-to-peer Game Engine

## Overview

This project is designed to be a foundation for future cross-platform turn-based multiplayer games.  The goal is for it to provide trait(s) that can be implemented by a specific game to easily add on cross-device network communication of game state and logic.

The idea is that for simple games between friends it is a lot of effort to set up a server somewhere to synchronise state between players. But the [Iroh Project](https://www.iroh.computer/) makes it much simpler to establish encrypted peer to peer networks directly between devices without the need for a middleman server *(beyond potentially a rendezvous server sometimes required for initially establishing connections)*.

## Features

- [x] **Core Game Logic Abstraction**: A `GameLogic` trait allows developers to plug in their own game rules, state, and actions.
- [x] **P2P Room Management**: Simple `create` and `join` functions for creating and joining game rooms using `iroh` tickets.
- [x] **Authoritative Host Model**: The host validates all actions and serves as the single source of truth for game state, preventing cheating.
- [x] **Lobby System**: Players can join a lobby, and all participants are notified of new arrivals before the game starts.
- [x] **Dynamic Role Assignment**: The `GameLogic` trait defines how roles (e.g., Player 1, Player 2, Observer) are assigned when the game starts.
- [x] **Real-time Event Loop**: An async event loop pushes game events (like state changes, new players, or chat messages) to the application.
- [x] **On-Demand State Queries**: Methods to pull the latest game state, player list, or app status at any time.
- [x] **Observer Mode**: Supports participants joining mid-game to watch without participating.
- [ ] **Built-in Chat**: A simple, real-time chat system for all participants.
- [ ] **Complete CLI Example**: A fully-functional Tic-Tac-Toe game demonstrates how to use the engine from end to end.
