/// EntryPoint v0.7 contract ABI bindings
pub mod v0_7 {
    use alloy_sol_macro::sol;

    sol! {
        /// Packed user operation as stored and processed by the EntryPoint
        #[derive(Debug, Default, PartialEq, Eq)]
        struct PackedUserOperation {
            address sender;
            uint256 nonce;
            bytes initCode;
            bytes callData;
            bytes32 accountGasLimits;
            uint256 preVerificationGas;
            bytes32 gasFees;
            bytes paymasterAndData;
            bytes signature;
        }

        /// Aggregated operations grouped by their signature aggregator
        #[derive(Debug, Default)]
        struct UserOpsPerAggregator {
            PackedUserOperation[] userOps;
            address aggregator;
            bytes signature;
        }

        /// Validation phase return data
        #[derive(Debug, Default)]
        struct ReturnInfo {
            uint256 preOpGas;
            uint256 prefund;
            uint256 accountValidationData;
            uint256 paymasterValidationData;
            bytes paymasterContext;
        }

        /// Staking information from the entry point
        #[derive(Debug, Default)]
        struct StakeInfo {
            uint256 stake;
            uint256 unstakeDelaySec;
        }

        /// Aggregator staking information
        #[derive(Debug, Default)]
        struct AggregatorStakeInfo {
            address aggregator;
            StakeInfo stakeInfo;
        }

        /// Combined validation result
        #[derive(Debug, Default)]
        struct ValidationResult {
            ReturnInfo returnInfo;
            StakeInfo senderInfo;
            StakeInfo factoryInfo;
            StakeInfo paymasterInfo;
            AggregatorStakeInfo aggregatorInfo;
        }

        /// Execution result from simulateHandleOp
        #[derive(Debug, Default)]
        struct ExecutionResult {
            uint256 preOpGas;
            uint256 paid;
            uint256 accountValidationData;
            uint256 paymasterValidationData;
            bool targetSuccess;
            bytes targetResult;
        }

        /// Deposit information for an address
        #[derive(Debug, Default)]
        struct DepositInfo {
            uint256 deposit;
            bool staked;
            uint112 stake;
            uint32 unstakeDelaySec;
            uint48 withdrawTime;
        }

        /// Core EntryPoint interface
        #[sol(rpc)]
        interface IEntryPoint {
            /// Execute a batch of user operations
            function handleOps(
                PackedUserOperation[] calldata ops,
                address payable beneficiary
            ) external;

            /// Execute aggregated user operations
            function handleAggregatedOps(
                UserOpsPerAggregator[] calldata opsPerAggregator,
                address payable beneficiary
            ) external;

            /// Get the hash of a user operation
            function getUserOpHash(
                PackedUserOperation calldata userOp
            ) external view returns (bytes32);

            /// Get the deposit balance for an address
            function balanceOf(address account) external view returns (uint256);

            /// Get deposit info for an address
            function getDepositInfo(address account) external view returns (DepositInfo memory info);

            /// Emitted for each successfully handled user operation
            event UserOperationEvent(
                bytes32 indexed userOpHash,
                address indexed sender,
                address indexed paymaster,
                uint256 nonce,
                bool success,
                uint256 actualGasCost,
                uint256 actualGasUsed
            );

            /// Emitted when an account is deployed by a user operation
            event AccountDeployed(
                bytes32 indexed userOpHash,
                address indexed sender,
                address factory,
                address paymaster
            );

            /// Error when a user operation fails validation or execution
            error FailedOp(uint256 opIndex, string reason);

            /// Error with inner revert data
            error FailedOpWithRevert(uint256 opIndex, string reason, bytes inner);

            /// Error when aggregator signature validation fails
            error SignatureValidationFailed(address aggregator);
        }

        /// Simulation-only EntryPoint interface (deployed as state override)
        #[sol(rpc)]
        interface IEntryPointSimulations {
            /// Simulate validation of a user operation
            function simulateValidation(
                PackedUserOperation calldata userOp
            ) external returns (ValidationResult memory);

            /// Simulate execution of a user operation
            function simulateHandleOp(
                PackedUserOperation calldata op,
                address target,
                bytes calldata targetCallData
            ) external returns (ExecutionResult memory);
        }
    }
}
