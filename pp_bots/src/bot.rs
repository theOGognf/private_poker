use anyhow::{bail, Error};
use private_poker::{
    entities::{ActionChoice, ActionChoices, GameView, SubHand, Usd, Usdf, Vote},
    functional,
    messages::{ClientMessage, ServerMessage, UserCommand, UserState},
    utils, Client,
};
use rand::{distributions::WeightedIndex, prelude::Distribution, thread_rng, Rng};
use std::{
    collections::{HashMap, HashSet},
    net::TcpStream,
    thread,
    time::Duration,
};

type State = Vec<SubHand>;
type ActionMasks = ActionChoices;
type ActionWeight = f32;
type ActionWeights = [f32; 5];
type Reward = f32;
type Done = bool;

const ACTION_OPTIONS_ARRAY: [ActionChoice; 5] = [
    ActionChoice::AllIn,
    ActionChoice::Call(0),
    ActionChoice::Check,
    ActionChoice::Fold,
    ActionChoice::Raise(0),
];
const Q_S_DEFAULT: ActionWeights = [0.2, 1.0, 1.0, 1.0, 0.2];

struct QLearningParams {
    alpha: f32,
    gamma: f32,
}

pub struct QLearning {
    params: QLearningParams,
    dist: WeightedIndex<ActionWeight>,
    table: HashMap<State, ActionWeights>,
}

impl QLearning {
    pub fn new(alpha: f32, gamma: f32) -> Self {
        Self {
            params: QLearningParams { alpha, gamma },
            dist: WeightedIndex::new(Q_S_DEFAULT).expect("weights should be valid"),
            table: HashMap::new(),
        }
    }

    pub fn sample(&mut self, state: State, masks: &ActionMasks) -> ActionChoice {
        let old_weights = self.table.entry(state).or_insert(Q_S_DEFAULT);
        let new_weights: Vec<ActionWeight> = ACTION_OPTIONS_ARRAY
            .iter()
            .enumerate()
            .map(|(idx, action_choice)| {
                if masks.contains(action_choice) {
                    old_weights[idx].exp()
                } else {
                    0.0
                }
            })
            .collect();
        let new_weights: Vec<(usize, &ActionWeight)> = new_weights.iter().enumerate().collect();
        self.dist
            .update_weights(&new_weights)
            .expect("weights should be valid");
        let action_idx = self.dist.sample(&mut thread_rng());
        let action_choice = &ACTION_OPTIONS_ARRAY[action_idx];
        masks
            .get(action_choice)
            .expect("action choice should be valid")
            .clone()
    }

    pub fn update_done(&mut self, state: State, action: ActionChoice, reward: Reward) {
        let q_s = self.table.entry(state).or_insert(Q_S_DEFAULT);
        let action_idx: usize = action.into();
        q_s[action_idx] = reward;
    }

    pub fn update_step(
        &mut self,
        state1: State,
        action: ActionChoice,
        reward: Reward,
        state2: State,
        masks2: ActionMasks,
    ) {
        let td_target = {
            let idx_masks2: HashSet<usize> =
                masks2.into_iter().map(std::convert::Into::into).collect();
            let q_s2 = self.table.entry(state2).or_insert(Q_S_DEFAULT);
            reward
                + self.params.gamma
                    * q_s2
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| idx_masks2.contains(idx))
                        .map(|(_, w)| w)
                        .max_by(|a, b| a.partial_cmp(b).expect("weights should be valid"))
                        .expect("weights should be valid")
        };
        let q_s1 = self.table.entry(state1).or_insert(Q_S_DEFAULT);
        let action_idx: usize = action.into();
        q_s1[action_idx] = q_s1[action_idx] + self.params.alpha * (td_target - q_s1[action_idx]);
    }
}

pub struct Bot {
    client: Client,
    hand: State,
    starting_money: Usd,
    view: GameView,
}

impl Bot {
    pub fn new(botname: &str, addr: &str) -> Result<Self, Error> {
        let (mut client, view) = Client::connect(botname, addr)?;
        let user = view
            .spectators
            .get(client.username.as_str())
            .expect("user should exist");
        client.stream.set_read_timeout(None)?;
        client.change_state(UserState::Play)?;
        Ok(Self {
            client,
            hand: vec![],
            starting_money: user.money,
            view,
        })
    }

    pub fn reset(&mut self) -> Result<(State, ActionMasks), Error> {
        // Hand is only empty on the first connection. Naturally, we'll be in
        // spectate when we first connect, so check that our hand isn't empty
        // before we try voting to reset ourself.
        if !self.hand.is_empty() && self.view.spectators.contains(self.client.username.as_str()) {
            // If we were moved to spectate, vote to reset ourself.
            let vote = Vote::Reset(Some(self.client.username.clone()));
            self.client.cast_vote(vote)?;
        }

        // Wait until it's our turn so we can get our hand and available
        // actions.
        let masks = loop {
            match utils::read_prefixed::<ServerMessage, TcpStream>(&mut self.client.stream) {
                // Bots are good sports and follow other votes.
                Ok(ServerMessage::Ack(ClientMessage {
                    username,
                    command: UserCommand::CastVote(vote),
                })) => {
                    if username != self.client.username {
                        match vote {
                            Vote::Kick(user_target) | Vote::Reset(Some(user_target))
                                if user_target == self.client.username => {}
                            _ => self.client.cast_vote(vote)?,
                        }
                    }
                }
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
                Err(error) => bail!(error),
                _ => {}
            }
        };
        Ok((self.hand.clone(), masks))
    }

    pub fn step(
        &mut self,
        action_choice: &ActionChoice,
    ) -> Result<(State, ActionMasks, Reward, Done), Error> {
        let player = self
            .view
            .players
            .iter()
            .find(|p| p.user.name == self.client.username)
            .expect("player should exist");
        // Sleep some random amount so real users have time to process info.
        let dur = Duration::from_secs(thread_rng().gen_range(1..8));
        thread::sleep(dur);
        let bet = match action_choice {
            ActionChoice::AllIn => player.user.money,
            ActionChoice::Check | ActionChoice::Fold => 0,
            ActionChoice::Call(amount) | ActionChoice::Raise(amount) => *amount,
        };
        self.client.take_action(action_choice.clone().into())?;
        if action_choice == &ActionChoice::Fold {
            return Ok((self.hand.clone(), HashSet::new(), 0.0, true));
        }
        let remaining_money = player.user.money - bet;
        let mut reward = -(bet as Usdf) / (self.starting_money as Usdf);
        // We have to wait until the game is over or wait until it's our turn
        // again so we can get masks and get the final reward for our action.
        let masks = loop {
            match utils::read_prefixed::<ServerMessage, TcpStream>(&mut self.client.stream) {
                // Bots are good sports and follow other votes.
                Ok(ServerMessage::Ack(ClientMessage {
                    username,
                    command: UserCommand::CastVote(vote),
                })) => {
                    if username != self.client.username {
                        self.client.cast_vote(vote)?;
                    }
                }
                Ok(ServerMessage::GameView(view)) => {
                    self.view = view;
                    if let Some(player) = self
                        .view
                        .players
                        .iter()
                        .find(|p| p.user.name == self.client.username)
                    {
                        // If we don't have anymore cards, then the game is over.
                        if player.cards.is_empty() {
                            reward += ((player.user.money - remaining_money) as Usdf)
                                / (self.starting_money as Usdf);
                            return Ok((self.hand.clone(), HashSet::new(), reward, true));
                        }
                        let mut cards = self.view.board.clone();
                        cards.extend(player.cards.clone());
                        functional::prepare_hand(&mut cards);
                        self.hand = functional::eval(&cards);
                    // We were forcibly moved to spectate because we don't have enough
                    // money. This means the current game is over.
                    } else if let Some(user) =
                        self.view.spectators.get(self.client.username.as_str())
                    {
                        reward += ((user.money - remaining_money) as Usdf)
                            / (self.starting_money as Usdf);
                        return Ok((self.hand.clone(), HashSet::new(), reward, true));
                    }
                }
                Ok(ServerMessage::TurnSignal(masks)) => break masks,
                Ok(ServerMessage::ClientError(error)) => bail!(error),
                Ok(ServerMessage::UserError(error)) => bail!(error),
                Err(error) => bail!(error),
                _ => {}
            }
        };
        Ok((self.hand.clone(), masks, reward, false))
    }
}
