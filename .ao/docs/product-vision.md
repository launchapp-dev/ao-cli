# Product Vision

## Project
- Name: ao-cli

## Problem
Manual orchestration of AI agents and CLI workflows is fragmented and unsafe, inconsistent, and hard to observe.

Refinement context: Manual AI-agent and CLI orchestration in software teams lacks a single deterministic control plane, causing unsafe operations, inconsistent execution outcomes, and limited end-to-end observability for planning, execution, QA, and audit.

## Target Users
- developers and automation operators using AO
- Platform/DevEx engineers standardizing internal AI-assisted delivery workflows
- SRE/operations teams requiring auditable, policy-enforced automation in CI and local environments
- Engineering managers and technical leads who need traceable execution evidence for governance and incident review

## Goals
- Provide a deterministic, local-first Rust AO CLI that unifies project planning, execution, review/QA, and audit trails under one control plane
- Increase operator confidence through explicit safety gates, reproducible run artifacts, and auditable state transitions
- Make AO fully machine-operable via structured JSON outputs for automation and policy enforcement
- Define measurable reliability targets (e.g., run reproducibility rate, failed destructive-action prevention rate, audit trace completeness)
- Prioritize phased delivery: core deterministic execution + state integrity first, then safety/policy controls, then advanced workflow ergonomics
- Establish interoperability goals for machine consumers (stable JSON schemas, backward-compatibility policy, and validation tooling)

## Constraints
- Keep the project Rust-only with no desktop-wrapper (Tauri) dependency
- Do not replace existing AO state files with non-machine-readable outputs
- Deliver command safety through confirmations for destructive operations and controlled destructive flows
- Ensure all runtime artifacts and history are traceable through `.ao/` state, run events, and task outputs
- Maintain backward compatibility for existing `.ao` state and JSON envelopes across minor releases
- Enforce deterministic behavior under partial failure and recovery paths (daemon restarts, orphan cleanup, interrupted runs)
- Bound AI-assisted behavior with explicit policy hooks and human-approval checkpoints for high-risk actions
- Require objective observability baselines (event completeness, error taxonomy coverage, and replayability) before expanding feature surface

## Value Proposition
AO provides a deterministic, inspectable, machine-friendly CLI control plane for orchestrating agent work from vision and requirements through execution and QA with auditable outcomes. AO is the trusted Rust-native control plane for AI-enabled software operations: deterministic by default, policy-aware for safety, and fully machine-consumable with auditable evidence from intent through delivery.

## Complexity
- Tier: complex
- Confidence: 0.76
- Recommended requirement range: 14-20
- Task density: high
- Rationale: Scope spans multi-surface orchestration (planning, runtime, QA, audit), strict determinism, safety-critical flows, schema stability, and AI-in-the-loop controls. Cross-cutting requirements for recoverability, observability, and policy enforcement increase coupling and validation load beyond medium complexity.
