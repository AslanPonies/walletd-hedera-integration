// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MyContract {
    string public message;

    constructor() {
        message = "Hello, Hedera!";
    }

    function setMessage(string memory newMessage) public {
        message = newMessage;
    }
}


