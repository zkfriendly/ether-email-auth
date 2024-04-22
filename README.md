# Ether Email-Auth SDK
## Overview
One issue with existing applications on Ethereum is that all users who execute transactions on-chain must install Ethereum-specific tools such as wallets and manage their own private keys.
Our ether email-auth SDK solves this issue: it allows users to execute any transaction on-chain simply by sending an email.

Using the SDK, a developer can build a smart contract with the following features without new ZKP circuits.
1. (Authorization) The contract can authorize any message in the Subject of the email that the user sends with a DKIM signature generated by an email provider, e.g., Gmail. 
2. (Authentication) The contract can authenticate that the given Ethereum address corresponds to the email address in the From field of the email.
3. (Privacy) No on-chain information reveals the user's email address itself. In other words, any adversary who learns only public data cannot estimate the corresponding email address from the Ethereum address.

One of its killer applications is email-based account recovery, social recovery for account abstraction (AA) wallets.
In social recovery, the wallet owner must appoint trusted persons as guardians who are authorized to update the private key for controlling the wallet.
However, not all such persons are necessarily Ethereum users.
Our solution mitigates this constraint by allowing guardians to complete the recovery process simply by sending an email.
In other words, any trusted persons can work as guardians as long as they can send emails.
Using the ether email-auth SDK, we construct a library and tools for any AA wallet providers to integrate our email-based account recovery just by implementing a few Solidity functions!

## Architecture
In addition to a user and a smart contract employing our SDK, there is a permissionless server called Relayer.
The Relayer connects the off-chain world, where the users are, with the on-chain world, where the contracts reside, without compromising security.
Specifically, the user, the Relayer, and the contract collaborate as follows:
1. (Off-chain) The user sends the Relayer an email containing a message to the contract in the Subject.
2. (Off-chain) The Relayer generates an email-auth message for the given email, consisting of data about the Subject, an Ethereum address corresponding to the user's email address, a ZK proof of the email, and so on.
3. (Off-chain -> On-chain) The Relayer broadcasts an Ethereum transaction to call the contract with the email-auth message.
4. (On-chain) After authorizing and authenticating the given email-auth message, the contract executes application-specific logic depending on the message in the Subject and the user's Ethereum address.
5. (On-chain -> Off-chain) The Relayer sends the user an email to report the execution result of the contract.

## Novel Concepts
### Account Code and Salt
An account code is a random integer in a finite scalar field of BN254 curve. 
It is a private randomness to derive a CREATE2 salt of the user’s Ethereum address from the email address, i.e., `userEtherAddr := CREATE2(hash(userEmailAddr, accountCode))`. 
That CREATE2 salt is called account salt, which is published on-chain.
**As long as the account code is hidden, no adversary can learn the user's email address from on-chain data.** 

### Invitation Code
An invitation code is a hex string of the account code along with a prefix, contained in any field of the email header to be inherited by its reply, e.g., Subject.
By confirming that a user sends an email with the invitation code, the contract can ensure that the account code is available to that user. 
It ensures the user’s liveness even when a malicious relayer or another user generates the user's account code because it prevents them from withholding the account code. 
**It suggests that the contract must check if the given email sent by the user contains the invitation code before confirming that user’s account for the first time.**

Notably, the email-auth message, which represents data in the user's email along with its ZK proof, has a boolean field `isCodeExist` such that the value is true if the invitation code is in the email.
However, the Subject message in the email-auth message masks characters for the invitation code.
**Consequently, no information beyond the existence of the invitation code is disclosed.**

### Subject Template
A subject template defines the expected format of the message in the Subject for each application.
**It allows developers to constrain that message to be in the application-specific format without new ZKP circuits.**

Specifically, the subject template is an array of strings, each of which has some fixed strings without space and the following variable parts:
- `"{string}"`: a string. Its Solidity type is `string`.
- `"{uint}"`: a decimal string of the unsigned integer. Its Solidity type is `uint256`.
- `"{int}"`: a decimal string of the signed integer. Its Solidity type is `int256`.
- `"{decimals}"`: a decimal string of the decimals. Its Solidity type is `uint256`. Its decimal size is fixed to 18. E.g., “2.7” ⇒ `abi.encode(2.7 * (10**18))`.
- `"{ethAddr}"`: a hex string of the Ethereum address. Its Solidity type is `address`. Its value MUST satisfy the checksum of the Ethereum address.

## How to Develop New Apps
There are four significant packages in this repo:
### `circuits` Package
It has a main circom circuit for verifying the email along with its DKIM signature, revealing a Subject message that masks an email address and an invitation code, and deriving an account salt from the email address in the From field and the given account code, which should match with the invitation code if it exists in the email.
The circuit is agnostic to application contexts such as subject templates.
**Therefore, a developer does not need to make new circuits.**

### `contracts` Package
It has Solidity contracts that help any smart contracts employing our SDK authorize and authenticate the email-auth message. Among them, there are three significant contracts: verifier, DKIM registry, and email-auth contracts.

The verifier contract in `Verifier.sol` has a responsibility to verify the ZK proof in the given email-auth message. 
It is a global contract, that is, multiple users in your application will use the same verifier contract.
You can deploy a new verifier contract once or use the already-deployed contract.

The DKIM registry contract has a responsibility to manage a mapping between an email domain name and its latest public keys used to verify the DKIM signatures. 
It should have mechanisms such that each user or some trusted custodians can rotate and void the registered public keys.
For example, the contract in `ECDSAOwnedDKIMRegistry.sol` requires ECDSA signatures from an owner of the predefined Ethereum address.
If you use the common trusted custodians for all users, you can deploy a new DKIM registry contract once or use the already-deployed contract.
If each user should be able to modify the registered public keys, a new DKIM registry contract needs to be deployed for each user.

The email-auth contract in `EmailAuth.sol` is a contract for each email user.
Its contract Ethereum address is calculated as the CREATE2 of the account salt, i.e., the hash of the user's email address and one account code held by the user. 
It provides an entry function `authEmail` to authorize and authenticate the email-auth message by calling the verifier and the DKIM registry contracts.
Besides, it supports [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271). 
After the email-auth message is processed in the `authEmail` function, the `isValidSignature` function of the email-auth contract returns true for the hash of the data in the email-auth message and its email nullifier, a 32-byte data unique to each email.

Your application contract can employ those contracts in the following manner:
1. For a new email user, the application contract deploys (a proxy of) the email-auth contract. Subsequently, the application contract sets the addresses of the verifier and the DKIM registry contracts and some subject templates for your application to the email-auth contract.
2. Given a new email-auth message from the email user, the application contract calls the `authEmail` function in the email-auth contract for that user. If it returns no error, the application contract can execute any processes based on the message in the email-auth message.

### `relayer` Package
It has a Rust implementation of the Relayer server.
Unfortunately, the current code only supports an application of the email-based account recovery described later.
We will provide a more generic implementation in the future.

### `prover` Package
It has some scripts for a prover server that generates a proof of the main circuit in the circuits package.
The Relayer calls the prover server for each given email to delegate the proof generation.
You can deploy the prover server either on your local machine or [Modal instances](https://modal.com/).

### Security Notes
Our SDK only performs the authorization and authentication of the email-auth message.
**You have a responsibility to ensure security and privacy in your application.**

Here, we present a list of security requirements that you should check.
- As described in the Subsection of "Invitation Code", for each email user, your application contract must ensure that the value of `isCodeExist` in the first email-auth message is true.
- The application contract can configure multiple subject templates for the same email-auth contract. However, the Relayer can choose any of the configured templates, as long as the message in the Subject matches with the chosen template. For example, if there are two templates "Send {decimals} {string}" and "Send {string}", the message "Send 1.23 ETH" matches with both templates. We recommend defining the subject templates without such ambiguities.
- To protect the privacy of the users' email addresses, you should carefully design not only the contracts but also the Relayer server. For example, if your Relayer storing the users' account codes exposes an API that returns the Ethereum address for the given email address and its stored account code, an adversary can breach that privacy. Additionally, if any Relayer's API returns an error when no account code is stored for the given email address, the adversary can learn which email addresses are registered.

## Application: Email-based Account Recovery
As a representative example of applications using our SDK, we provide contracts and a Relayer server for email-based account recovery. They assume a life cycle of the account recovery in four phases:
1. (Requesting a guardian) A wallet owner requests a holder of a specified email address to become a guardian.
2. (Accepting a guardian) If the requested guardian sends an email to accept the request, the wallet contract registers the Ethereum address corresponding to its email address.
3. (Processing a recovery for each guardian) When a guardian sends an email to recover the wallet, the wallet contract updates state data for the recovery.
4. (Completing a recovery) If the required condition for the recovery holds, the account recovery is done.

Specifically, we expect the following UX. **Notably, the guardian only needs to reply to emails sent from the Relayer in every process.**
1. (Requesting a guardian 1/4) A web page to configure guardians for email-based account recovery requests the wallet owner to input the guardian's email address and some related data such as the length of timelock until that guardian's recovery request is enabled. 
2. (Requesting a guardian 2/4) The frontend script on the web page randomly generates a new account code and derives the guardian's Ethereum address from the input guardian's email address and that code. 
It then requests the wallet owner to broadcast a transaction to register the guardian request into the wallet contract, passing the derived Ethereum address and the related data (not the private data such as the email address and the account code).
3. (Requesting a guardian 3/4) The frontend script also sends the wallet address, the guardian's email address, and the account code to the Relayer. 
4. (Requesting a guardian 4/4) The Relayer then sends the guardian an email to say "The owner of this wallet address requests you to become a guardian".
5. (Accepting a guardian 1/3) If confirming the request, the guardian replies to the Relayer's email.
6. (Accepting a guardian 2/3) The Relayer generates an email-auth message for the guardian's email and then broadcasts a transaction to pass it to the wallet contract.
7. (Accepting a guardian 3/3) If the given email-auth message is valid, the wallet contract deploys an email-auth contract for the guardian and stores its address as the guardian.
8. (Processing a recovery for each guardian 1/6) When losing a private key for controlling the wallet, on the web page to recover the wallet, the wallet owner inputs the wallet address to be recovered, the guardian's email address and a new EOA address called owner address derived from a fresh private key.
9.  (Processing a recovery for each guardian 2/6) The frontend script on the web page sends the input data to the Relayer.
10. (Processing a recovery for each guardian 3/6) The Relayer then sends the guardian an email to say "Please rotate the owner address for this wallet address to this EOA address".
11. (Processing a recovery for each guardian 4/6) If confirming the requested recovery, the guardian replies to the Relayer's email.
12. (Processing a recovery for each guardian 5/6) The Relayer generates an email-auth message for the guardian's email and then broadcasts a transaction to pass it to the wallet contract.
13. (Processing a recovery for each guardian 6/6) If the given email-auth message is valid, the wallet contract updates states for the recovery.
14. (Completing a recovery 1/) When the frontend script finds that the required condition to complete the recovery holds on-chain, e.g., enough number of the guardian's confirmations are registered into the wallet contract, it requests the Relayer to complete the recovery.
15. (Completing a recovery 1/) The Relayer broadcasts a transaction to call a function in the wallet contract for completing the recovery. If it returns no error, the owner address should be rotated.

<!-- The above life cycle can support various practical implementations of account recovery. For example, if your wallet contract requires confirmations from multiple guardians and sets a timelock if only less than three guardians confirm the recovery, you can implement such functions as follows:
1. (Setting a guardian) Your wallet contract stores a list of multiple guardians' Ethereum addresses. When the wallet approves a new guardian’s email, it deploys a new 
 adds the guardian’s Ethereum address to that list.
2. When the wallet approves one guardian’s email for the first time, it sets the status of the wallet to a recovering mode and stores the new signer address in the email, the guardian’s ethereum address, and the block timestamp. After that, every time the wallet approves a guardian’s email, it adds the guardian’s ethereum address to the confirming guardians list.
3. If the size of the confirming guardians list is two, the wallet updates the signer address if and only if the timelock is expired. Otherwise, i.e., the number of the confirming guardians are more than two, the wallet immediately updates it.
-->



