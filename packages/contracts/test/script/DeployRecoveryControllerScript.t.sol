// SPDX-License-Identifier: MIT
pragma solidity ^0.8.12;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import {Deploy} from "../../script/DeployRecoveryController.s.sol";
import {StructHelper} from "../helpers/StructHelper.sol";

contract DeployRecoveryControllerScriptTest is StructHelper {
    function setUp() public override {
        vm.setEnv(
            "PRIVATE_KEY",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        );
        vm.setEnv("SIGNER", "0x69bec2dd161d6bbcc91ec32aa44d9333ebc864c0");
    }

    function test_run() public {
        skipIfZkSync();

        Deploy deploy = new Deploy();
        deploy.run();
        require(
            vm.envAddress("ECDSA_DKIM") != address(0),
            "ECDSA_DKIM is not set"
        );
        require(vm.envAddress("VERIFIER") != address(0), "VERIFIER is not set");
        require(
            vm.envAddress("EMAIL_AUTH_IMPL") != address(0),
            "EMAIL_AUTH_IMPL is not set"
        );
        require(
            vm.envAddress("RECOVERY_CONTROLLER") != address(0),
            "RECOVERY_CONTROLLER is not set"
        );
        require(
            vm.envAddress("SIMPLE_WALLET") != address(0),
            "SIMPLE_WALLET is not set"
        );
    }
}
