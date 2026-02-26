# cdd-rust Architecture

The internal architecture separates the core AST/OpenAPI parsing logic from the target code generation. This allows the
tool to support multiple web frameworks and ORMs through the `BackendStrategy` and `ModelMapper` traits.

```mermaid
graph LR
%% --- NODES --- 
    InputOAS(<strong>OpenAPI Spec</strong><br/><em>YAML</em>)
    InputSrc(<strong>Rust Source</strong><br/><em>Files / Schema</em>)

    subgraph Core [Layer 1: Core]
        direction TB
        P_OAS(<strong>OAS Parser</strong><br/><em>serde_yaml</em>)
        P_AST(<strong>AST Parser</strong><br/><em>ra_ap_syntax</em>)
    end

    subgraph Analysis [Layer 2: Analysis]
        IR(<strong>Intermediate Representation</strong><br/><em>ParsedRoute / ParsedStruct</em>)
    end

    subgraph Gen [Layer 3: Generation]
        Base(<strong>Generator Engine</strong><br/><em>Traits: BackendStrategy & ModelMapper</em>)

    %% The Fork
        subgraph Targets [Targets]
            direction TB
            T_Actix(<strong>Actix</strong><br/><em>ActixStrategy</em>)
            T_Diesel(<strong>Diesel</strong><br/><em>DieselMapper</em>)
            T_OutputOAS(<strong>OpenAPI</strong><br/><em>Spec Generation</em>)
            T_Future(<strong>Axum / SQLx</strong><br/><em>Future Strategies</em>)
        end
    end

%% --- EDGES --- 
    InputOAS --> P_OAS
    InputSrc --> P_AST

    P_OAS --> IR
    P_AST --> IR

    IR --> Base

    Base -- "Scaffold / Test" --> T_Actix
    Base -- "Sync Models" --> T_Diesel
    Base -- "Reflect" --> T_OutputOAS
    Base -. "Extension" .-> T_Future

%% --- STYLING --- 
    classDef blue fill:#4285f4,stroke:#ffffff,color:#ffffff,stroke-width:0px
    classDef yellow fill:#f9ab00,stroke:#ffffff,color:#20344b,stroke-width:0px
    classDef green fill:#34a853,stroke:#ffffff,color:#ffffff,stroke-width:0px
    classDef white fill:#ffffff,stroke:#20344b,color:#20344b,stroke-width:2px
    classDef future fill:#f1f3f4,stroke:#20344b,color:#20344b,stroke-width:2px,stroke-dasharray: 5 5

    class InputOAS,InputSrc white
    class P_OAS,P_AST blue
    class IR yellow
    class Base green
    class T_Actix,T_Diesel,T_OutputOAS white
    class T_Future future
```

The project is workspace-based to separate core logic from the command-line interface.

| Crate          | Purpose                                                                                                                                                                                    |
|----------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **`cdd-core`** | **The Engine.** Contains the `ra_ap_syntax` parsers, the OpenAPI 3.x parser (with 3.2 shims), AST diffing logic, and the Backend Strategy traits (currently implementing `ActixStrategy`). |
| **`cdd-cli`**  | **The Interface.** Provides the `sync`, `scaffold`, `schema-gen` and `test-gen` commands.                                                                                                  |
| **`cdd-web`**  | **The Reference.** An Actix+Diesel implementation demonstrating the generated code and tests in action.                                                                                    |

## Testing and Compliance

The codebase is strictly enforced to achieve **100% test coverage** and **100% documentation coverage** without relying on configuration bypasses (such as `tarpaulin.toml` exceptions or injected `#![allow(missing_docs)]` pragmas). Continuous Integration (CI) enforces these targets.
