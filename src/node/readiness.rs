//! Test-only characterization of the current startup lifecycle.
//!
//! This module deliberately is not wired into production. It records the
//! transition boundaries that the current startup path can prove so a later
//! tranche can introduce runtime propagation without inventing observations.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComponentState {
    Starting,
    Ready,
    Degraded,
    NotReady,
    Failed,
    Disabled,
}

impl ComponentState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::NotReady => "not_ready",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupPhase {
    ProcessLaunched,
    ConfigurationLoaded,
    ChainReady,
    P2pServicesStarted,
    HttpBound,
    Operational,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupEvent {
    ConfigurationLoaded,
    ChainRecovered,
    P2pServicesStarted,
    HttpListenerBound,
    ServicesReportedStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FatalStartupBoundary {
    Configuration,
    ChainInitialization,
    P2pListenerBind,
    HttpListenerBind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedDialObservation {
    NotConfigured,
    ConfiguredNotSpawned,
    TasksSpawned,
    AttemptBegun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InvalidTransition {
    phase: StartupPhase,
    event: StartupEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InvalidFailureTransition {
    phase: StartupPhase,
    boundary: FatalStartupBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeOperationalState {
    phase: StartupPhase,
    lifecycle: ComponentState,
    configuration: ComponentState,
    chain: ComponentState,
    p2p_listener: ComponentState,
    http_listener: ComponentState,
    mining: ComponentState,
    seed_dial: SeedDialObservation,
    fatal_boundary: Option<FatalStartupBoundary>,
}

impl NodeOperationalState {
    fn launched(mining_enabled: bool, seeds_configured: bool) -> Self {
        Self {
            phase: StartupPhase::ProcessLaunched,
            lifecycle: ComponentState::Starting,
            configuration: ComponentState::Starting,
            chain: ComponentState::Starting,
            p2p_listener: ComponentState::Starting,
            http_listener: ComponentState::Starting,
            mining: if mining_enabled {
                ComponentState::Starting
            } else {
                ComponentState::Disabled
            },
            seed_dial: if seeds_configured {
                SeedDialObservation::ConfiguredNotSpawned
            } else {
                SeedDialObservation::NotConfigured
            },
            fatal_boundary: None,
        }
    }

    fn advance(&mut self, event: StartupEvent) -> Result<(), InvalidTransition> {
        let valid = matches!(
            (self.phase, event),
            (
                StartupPhase::ProcessLaunched,
                StartupEvent::ConfigurationLoaded
            ) | (
                StartupPhase::ConfigurationLoaded,
                StartupEvent::ChainRecovered
            ) | (StartupPhase::ChainReady, StartupEvent::P2pServicesStarted)
                | (
                    StartupPhase::P2pServicesStarted,
                    StartupEvent::HttpListenerBound
                )
                | (
                    StartupPhase::HttpBound,
                    StartupEvent::ServicesReportedStarted
                )
        );

        if !valid {
            return Err(InvalidTransition {
                phase: self.phase,
                event,
            });
        }

        match event {
            StartupEvent::ConfigurationLoaded => {
                self.phase = StartupPhase::ConfigurationLoaded;
                self.configuration = ComponentState::Ready;
            }
            StartupEvent::ChainRecovered => {
                self.phase = StartupPhase::ChainReady;
                self.chain = ComponentState::Ready;
            }
            StartupEvent::P2pServicesStarted => {
                self.phase = StartupPhase::P2pServicesStarted;
                self.p2p_listener = ComponentState::Ready;
                if self.seed_dial == SeedDialObservation::ConfiguredNotSpawned {
                    self.seed_dial = SeedDialObservation::TasksSpawned;
                }
            }
            StartupEvent::HttpListenerBound => {
                self.phase = StartupPhase::HttpBound;
                self.http_listener = ComponentState::Ready;
            }
            StartupEvent::ServicesReportedStarted => {
                self.phase = StartupPhase::Operational;
                self.lifecycle = ComponentState::Ready;
            }
        }

        Ok(())
    }

    fn record_fatal(
        &mut self,
        boundary: FatalStartupBoundary,
    ) -> Result<(), InvalidFailureTransition> {
        let valid = matches!(
            (self.phase, boundary),
            (
                StartupPhase::ProcessLaunched,
                FatalStartupBoundary::Configuration
            ) | (
                StartupPhase::ConfigurationLoaded,
                FatalStartupBoundary::ChainInitialization
            ) | (
                StartupPhase::ChainReady,
                FatalStartupBoundary::P2pListenerBind
            ) | (
                StartupPhase::P2pServicesStarted,
                FatalStartupBoundary::HttpListenerBind
            )
        );

        if !valid {
            return Err(InvalidFailureTransition {
                phase: self.phase,
                boundary,
            });
        }

        self.phase = StartupPhase::Failed;
        self.lifecycle = ComponentState::Failed;
        self.fatal_boundary = Some(boundary);
        match boundary {
            FatalStartupBoundary::Configuration => {
                self.configuration = ComponentState::Failed;
            }
            FatalStartupBoundary::ChainInitialization => {
                self.chain = ComponentState::Failed;
            }
            FatalStartupBoundary::P2pListenerBind => {
                self.p2p_listener = ComponentState::Failed;
            }
            FatalStartupBoundary::HttpListenerBind => {
                self.http_listener = ComponentState::Failed;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance_to_chain_ready(state: &mut NodeOperationalState) {
        state.advance(StartupEvent::ConfigurationLoaded).unwrap();
        state.advance(StartupEvent::ChainRecovered).unwrap();
    }

    fn advance_to_p2p_started(state: &mut NodeOperationalState) {
        advance_to_chain_ready(state);
        state.advance(StartupEvent::P2pServicesStarted).unwrap();
    }

    #[test]
    fn component_state_vocabulary_matches_approved_policy() {
        assert_eq!(ComponentState::Starting.as_str(), "starting");
        assert_eq!(ComponentState::Ready.as_str(), "ready");
        assert_eq!(ComponentState::Degraded.as_str(), "degraded");
        assert_eq!(ComponentState::NotReady.as_str(), "not_ready");
        assert_eq!(ComponentState::Failed.as_str(), "failed");
        assert_eq!(ComponentState::Disabled.as_str(), "disabled");
    }

    #[test]
    fn current_happy_path_reaches_operational_in_observed_order() {
        let mut state = NodeOperationalState::launched(false, true);

        assert_eq!(state.phase, StartupPhase::ProcessLaunched);
        assert_eq!(state.lifecycle, ComponentState::Starting);
        assert_eq!(state.mining, ComponentState::Disabled);
        assert_eq!(state.seed_dial, SeedDialObservation::ConfiguredNotSpawned);

        state.advance(StartupEvent::ConfigurationLoaded).unwrap();
        assert_eq!(state.configuration, ComponentState::Ready);

        state.advance(StartupEvent::ChainRecovered).unwrap();
        assert_eq!(state.chain, ComponentState::Ready);

        state.advance(StartupEvent::P2pServicesStarted).unwrap();
        assert_eq!(state.p2p_listener, ComponentState::Ready);
        assert_eq!(state.seed_dial, SeedDialObservation::TasksSpawned);

        state.advance(StartupEvent::HttpListenerBound).unwrap();
        assert_eq!(state.http_listener, ComponentState::Ready);

        state
            .advance(StartupEvent::ServicesReportedStarted)
            .unwrap();
        assert_eq!(state.phase, StartupPhase::Operational);
        assert_eq!(state.lifecycle, ComponentState::Ready);
        assert_eq!(state.fatal_boundary, None);
    }

    #[test]
    fn current_seed_boundary_does_not_claim_connection_attempt_began() {
        let mut state = NodeOperationalState::launched(false, true);
        advance_to_p2p_started(&mut state);

        assert_eq!(state.seed_dial, SeedDialObservation::TasksSpawned);
        assert_ne!(state.seed_dial, SeedDialObservation::AttemptBegun);
    }

    #[test]
    fn no_seed_configuration_remains_explicitly_distinct() {
        let mut state = NodeOperationalState::launched(false, false);
        advance_to_p2p_started(&mut state);

        assert_eq!(state.seed_dial, SeedDialObservation::NotConfigured);
    }

    #[test]
    fn mining_task_start_does_not_claim_mining_is_ready() {
        let mut state = NodeOperationalState::launched(true, false);
        advance_to_p2p_started(&mut state);
        state.advance(StartupEvent::HttpListenerBound).unwrap();
        state
            .advance(StartupEvent::ServicesReportedStarted)
            .unwrap();

        assert_eq!(state.lifecycle, ComponentState::Ready);
        assert_eq!(state.mining, ComponentState::Starting);
    }

    #[test]
    fn out_of_order_transition_is_rejected_without_mutation() {
        let mut state = NodeOperationalState::launched(false, false);
        let before = state.clone();

        let error = state.advance(StartupEvent::ChainRecovered).unwrap_err();

        assert_eq!(
            error,
            InvalidTransition {
                phase: StartupPhase::ProcessLaunched,
                event: StartupEvent::ChainRecovered,
            }
        );
        assert_eq!(state, before);
    }

    #[test]
    fn characterized_fatal_boundaries_mark_the_affected_component() {
        let mut configuration = NodeOperationalState::launched(false, false);
        configuration
            .record_fatal(FatalStartupBoundary::Configuration)
            .unwrap();
        assert_eq!(configuration.configuration, ComponentState::Failed);

        let mut chain = NodeOperationalState::launched(false, false);
        chain.advance(StartupEvent::ConfigurationLoaded).unwrap();
        chain
            .record_fatal(FatalStartupBoundary::ChainInitialization)
            .unwrap();
        assert_eq!(chain.chain, ComponentState::Failed);

        let mut p2p = NodeOperationalState::launched(false, false);
        advance_to_chain_ready(&mut p2p);
        p2p.record_fatal(FatalStartupBoundary::P2pListenerBind)
            .unwrap();
        assert_eq!(p2p.p2p_listener, ComponentState::Failed);
        assert_eq!(p2p.lifecycle, ComponentState::Failed);
        assert_eq!(
            p2p.fatal_boundary,
            Some(FatalStartupBoundary::P2pListenerBind)
        );

        let mut http = NodeOperationalState::launched(false, false);
        advance_to_p2p_started(&mut http);
        http.record_fatal(FatalStartupBoundary::HttpListenerBind)
            .unwrap();
        assert_eq!(http.http_listener, ComponentState::Failed);
    }

    #[test]
    fn fatal_boundary_is_rejected_outside_its_characterized_phase() {
        let mut state = NodeOperationalState::launched(false, false);
        let before = state.clone();

        let error = state
            .record_fatal(FatalStartupBoundary::HttpListenerBind)
            .unwrap_err();

        assert_eq!(
            error,
            InvalidFailureTransition {
                phase: StartupPhase::ProcessLaunched,
                boundary: FatalStartupBoundary::HttpListenerBind,
            }
        );
        assert_eq!(state, before);
    }
}
