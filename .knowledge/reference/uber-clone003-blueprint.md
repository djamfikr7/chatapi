# Uber-Clone003 Master Blueprint (Reference)

**Source:** `/media/fi/NewVolume10/project01/Uber-Clone003/AGENTS.md`
**Purpose:** Reference blueprint for agentic parallel development patterns, event sourcing, and observability

## Key Patterns Applicable to ChatAPI

### 1. Parallel Agentic Framework (PAF)
- Git Worktree Isolation for parallel agents
- AST Merging for code integration
- Ephemeral Sandboxes for testing before PR

### 2. Vault-First Knowledge Management
- All architecture decisions as ADRs in versioned markdown
- Knowledge base is the "brain" — code and docs inseparable
- Dynamic sync protocol for agent knowledge updates

### 3. Glass-Box Observability
- Real-time telemetry to Developer Command Center
- Swarm Topology: agent activity visualization
- All background tasks emit telemetry

### 4. Event-Sourced Architecture
- Double-entry bookkeeping for financial flows
- NATS JetStream for cross-service communication
- CQRS for read/write separation

### 5. Simulation-Driven Optimization
- SEAO closed-loop: Seed → Simulate → Critique → Mutate → Converge
- Standard simulation scenarios ("Buttons") for stress testing
- Fitness functions for architecture validation

## Relevance to ChatAPI
- **PAF patterns** → parallel agent dispatch for independent crates
- **Vault-first** → .knowledge/ directory as our versioned wiki
- **Observability** → gateway logs endpoint, streaming events
- **Event sourcing** → session history, tool execution audit trail
