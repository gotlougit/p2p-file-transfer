# P2P File Transfer

## Overview

This is a small project to help solve a problem that should've been solved by now: file transfer.

For in-person file transfers, we have lots of solutions: Google's Nearby Share and Apple's Airdrop are good examples. But for long-distance ones, the best we've been able to do is just upload the content to a third-party server for everyone to download from. This is simply wasteful and costly.

Why not just put the huge amount of resources residential Internet users get to use and DIY it?

## Tech

It is being coded in Rust in order to get both safety and performance, using the Tokio asynchronous framework in order to get faster performance.

Both the client and server have async network request support.

## Implemented Features

- Custom UDP-based basic protocol to transfer files as well as provide groundwork for implementing encryption support.

- NAT traversal code which temporarily opens up ports between client and server to communicate with each other.

- A very very basic level of support for authentication and authorization. This will become more important after encryption is implemented and the program is ready for general usage.

- Tests for the protocol and raw networking code using a dummy socket interface to avoid creating separate OS sockets and complicating things

## Goals

- Encryption: almost every single communication should be completely encrypted, so there is no way to tell what file is being transferred.

- Send client and server data along with public keys in a user-friendly way: just have both parties send a small code which encodes all this info so they don't have to type it out. The exact details can be worked out later.

- Send multiple files at once. This will require some additional handshaking implemented in the custom protocol

- Server should be able to perform NAT traversal for a new client that wants to connect to a running instance instead of having to run another instance of the program

## Status

Alpha: I am basically learning network programming through this. This is in NO way usable right now.

Right now, it is able to make direct connections through easy NAT and transfer one file from one machine (called a server) to another machine (called a client) over the Internet.

Thanks to implementing Go Back N (albeit not in a traditional fashion), it is able to send files with fairly good speeds, although this does need major improvements
