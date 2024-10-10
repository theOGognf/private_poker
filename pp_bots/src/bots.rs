use anyhow::{bail, Error};
use private_poker::{
    entities::{Action, SubHand, Usd, Usdf},
    functional,
    messages::{GameView, ServerMessage, UserState},
    Client,
};
use rand::{distributions::WeightedIndex, prelude::Distribution, thread_rng};
use std::collections::{HashMap, HashSet};

const ACTIONS_ARRAY: [Action; 5] = [
    Action::AllIn,
    Action::Call(0),
    Action::Check,
    Action::Fold,
    Action::Raise(0),
];
const Q_S_DEFAULT: [f32; 5] = [1.0; 5];

type State = Vec<SubHand>;
type ActionMask = HashSet<Action>;
type Reward = f32;
type Done = bool;

struct QParams {
    alpha: f32,
    gamma: f32,
}

struct Q {
    params: QParams,
    dist: WeightedIndex<f32>,
    table: HashMap<Vec<SubHand>, [f32; 5]>,
}

impl Q {
    fn new(alpha: f32, gamma: f32) -> Self {
        Self {
            params: QParams { alpha, gamma },
            dist: WeightedIndex::new(Q_S_DEFAULT).expect("valid weights"),
            table: HashMap::new(),
        }
    }

    fn sample(&mut self, state: State, masks: ActionMask) -> Action {
        let old_weights = self.table.entry(state).or_insert(Q_S_DEFAULT);
        let new_weights: Vec<f32> = ACTIONS_ARRAY
            .iter()
            .enumerate()
            .map(|(idx, action)| {
                if masks.contains(action) {
                    old_weights[idx].exp()
                } else {
                    0.0
                }
            })
            .collect();
        let new_weights: Vec<(usize, &f32)> = new_weights.iter().enumerate().collect();
        self.dist
            .update_weights(&new_weights)
            .expect("valid weights");
        let action_idx = self.dist.sample(&mut thread_rng());
        let action = &ACTIONS_ARRAY[action_idx];
        masks.get(action).expect("valid action").clone()
    }

    fn update_done(&mut self, state: State, action: Action, reward: Reward) {
        let q_s = self.table.entry(state).or_insert(Q_S_DEFAULT);
        let action_idx: usize = action.into();
        q_s[action_idx] = reward;
    }

    fn update_step(
        &mut self,
        state1: State,
        action: Action,
        reward: Reward,
        state2: State,
        masks2: ActionMask,
    ) {
        let td_target = {
            let idx_masks2: HashSet<usize> = masks2.into_iter().map(|a| a.into()).collect();
            let q_s2 = self.table.entry(state2).or_insert(Q_S_DEFAULT);
            reward
                + self.params.gamma
                    * q_s2
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| idx_masks2.contains(idx))
                        .map(|(_, w)| w)
                        .max_by(|a, b| a.partial_cmp(b).expect("valid weights"))
                        .expect("valid weights")
        };
        let q_s1 = self.table.entry(state1).or_insert(Q_S_DEFAULT);
        let action_idx: usize = action.into();
        q_s1[action_idx] = q_s1[action_idx] + self.params.alpha * (td_target - q_s1[action_idx]);
    }
}

struct PokerEnv {
    client: Client,
    hand: State,
    starting_money: Usd,
    view: GameView,
}

impl PokerEnv {
    fn new(username: &str, addr: &str) -> Result<Self, Error> {
        let (mut client, view) = Client::connect(username, addr)?;
        let user = view
            .waitlist
            .iter()
            .find(|u| u.name == username)
            .expect("user exists");
        client.change_state(UserState::Play)?;
        Ok(Self {
            client,
            hand: vec![],
            starting_money: user.money,
            view,
        })
    }

    fn reset(&mut self) -> Result<(State, ActionMask), Error> {
        // Check if we have enough for the big blind. If we don't, we have
        // to wait until we're moved to spectator and then disconnect,
        // reconnect, and then move to the waitlist.
        if let Some(player) = self
            .view
            .players
            .iter()
            .find(|p| p.user.name == self.client.username)
        {
            if player.user.money < self.view.big_blind {
                loop {
                    match self.client.recv() {
                        Ok(ServerMessage::GameView(view)) => {
                            if view.spectators.contains_key(&self.client.username) {
                                self.client.stream.shutdown(std::net::Shutdown::Both).ok();
                                let (mut client, view) =
                                    Client::connect(&self.client.username, &self.client.addr)?;
                                client.change_state(UserState::Play)?;
                                self.client = client;
                                self.view = view;
                            }
                        }
                        Ok(ServerMessage::ClientError(error)) => bail!(error),
                        Ok(ServerMessage::UserError(error)) => bail!(error),
                        // Don't care about acks, game statuses, or turn signals.
                        Ok(_) => {}
                        Err(error) => bail!(error),
                    }
                }
            }
        }

        // Wait until it's our turn.
        let masks = loop {
            match self.client.recv() {
                Ok(ServerMessage::GameView(view)) => {
                    self.view = view;
                    if let Some(player) = self
                        .view
                        .players
                        .iter()
                        .find(|p| p.user.name == self.client.username)
                    {
                        let mut cards = self.view.board.clone();
                        cards.extend(player.cards.clone());
                        functional::prepare_hand(&mut cards);
                        self.hand = functional::eval(&cards);
                        self.starting_money = player.user.money;
                    }
                }
                Ok(ServerMessage::TurnSignal(masks)) => break masks,
                Ok(ServerMessage::ClientError(error)) => bail!(error),
                Ok(ServerMessage::UserError(error)) => bail!(error),
                // Don't care about acks or game statuses.
                Ok(_) => {}
                Err(error) => bail!(error),
            }
        };

        Ok((self.hand.clone(), masks))
    }

    fn step(&mut self, action: Action) -> Result<(State, ActionMask, Reward, Done), Error> {
        let player = self
            .view
            .players
            .iter()
            .find(|p| p.user.name == self.client.username)
            .expect("player exists");
        let bet = match action {
            Action::AllIn => player.user.money,
            Action::Check => 0,
            Action::Fold => return Ok((self.hand.clone(), HashSet::new(), 0.0, true)),
            Action::Call(amount) => amount,
            Action::Raise(amount) => amount,
        };
        self.client.take_action(action)?;
        let remaining_money = player.user.money - bet;
        let mut reward = -(bet as Usdf) / (self.starting_money as Usdf);
        // We have to wait until the game is over or wait until it's our turn
        // again so we can get masks and get the final reward for our action.
        let masks = loop {
            match self.client.recv() {
                Ok(ServerMessage::GameView(view)) => {
                    // If we don't have anymore cards, then the game is over.
                    self.view = view;
                    let player = self
                        .view
                        .players
                        .iter()
                        .find(|p| p.user.name == self.client.username)
                        .expect("player exists");
                    if player.cards.is_empty() {
                        // Reward is relative to money at the start of the game
                        // and to money after the last action was made.
                        reward += ((player.user.money - remaining_money) as Usdf)
                            / (self.starting_money as Usdf);
                        return Ok((self.hand.clone(), HashSet::new(), reward, true));
                    } else {
                        let mut cards = self.view.board.clone();
                        cards.extend(player.cards.clone());
                        functional::prepare_hand(&mut cards);
                        self.hand = functional::eval(&cards);
                    }
                }
                Ok(ServerMessage::TurnSignal(masks)) => break masks,
                Ok(ServerMessage::ClientError(error)) => bail!(error),
                Ok(ServerMessage::UserError(error)) => bail!(error),
                // Don't care about acks or game statuses.
                Ok(_) => {}
                Err(error) => bail!(error),
            }
        };
        Ok((self.hand.clone(), masks, reward, false))
    }
}

pub fn run(username: &str, addr: &str) -> Result<(), Error> {
    let mut policy = Q::new(0.1, 0.95);
    let mut env = PokerEnv::new(username, addr)?;
    loop {
        let (mut state1, mut masks1) = env.reset()?;
        loop {
            let action = policy.sample(state1.clone(), masks1.clone());
            let (state2, masks2, reward, done) = env.step(action.clone())?;
            if done {
                policy.update_done(state1.clone(), action.clone(), reward);
                break;
            }
            policy.update_step(
                state1.clone(),
                action.clone(),
                reward,
                state2.clone(),
                masks2.clone(),
            );
            state1.clone_from(&state2);
            masks1.clone_from(&masks2);
        }
    }
}
