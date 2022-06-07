pub mod region;
pub mod behavior;
pub mod nodes;
pub mod nodes_utility;
pub mod nodes_area;
pub mod nodes_game;
pub mod script;
pub mod game;
pub mod game_screen;

use core_shared::characterdata::CharacterData;
use core_shared::regiondata::GameRegionData;
use core_shared::update::GameUpdate;
use core_shared::asset::{ TileUsage, Asset };
use core_shared::actions::*;

use rhai::{ Engine, Scope, AST };

use std::collections::{HashMap, HashSet};
use std::fs::metadata;
use std::hash::Hash;

use crate::gamedata::region::GameRegion;
use crate::gamedata::behavior::{ BehaviorNodeConnector, BehaviorInstance, GameBehavior, BehaviorNodeType, BehaviorType, BehaviorInstanceState };
use crate::draw2d::Draw2D;

use itertools::Itertools;

use std::path::{self, PathBuf};
use std::fs;

use rand::prelude::*;

use self::behavior::BehaviorInstanceType;
use self::game::Game;
use self::game_screen::GameScreen;
use self::nodes_utility::get_node_value;

use crate::script_types::*;

type NodeCall = fn(instance_index: usize, id: (usize, usize), data: &mut GameData, behavior_type: BehaviorType) -> behavior::BehaviorNodeConnector;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum MessageType {
    Status,
    Say,
    Yell,
    Private,
    Debug,
    Error,
}

pub struct GameData<'a> {

    pub asset                   : Option<Asset>,
    pub draw2d                  : Option<Draw2D>,

    pub path                    : PathBuf,

    pub regions                 : HashMap<usize, GameRegion>,
    pub regions_names           : Vec<String>,
    pub regions_ids             : Vec<usize>,

    pub behaviors               : HashMap<usize, GameBehavior>,
    pub behaviors_names         : Vec<String>,
    pub behaviors_ids           : Vec<usize>,

    pub systems                 : HashMap<usize, GameBehavior>,
    pub systems_names           : Vec<String>,
    pub systems_ids             : Vec<usize>,

    pub items                   : HashMap<usize, GameBehavior>,
    pub items_names             : Vec<String>,
    pub items_ids               : Vec<usize>,

    pub game                    : Game,

    /// The index of the game instance
    pub game_instance_index     : Option<usize>,

    // Game screen dimensions
    pub game_screen_width       : usize,
    pub game_screen_height      : usize,
    pub game_screen_tile_size   : usize,

    pub game_screens            : HashMap<usize, GameScreen>,

    pub game_anim_counter       : usize,
    pub game_frame              : Vec<u8>,

    pub nodes                   : HashMap<BehaviorNodeType, NodeCall>,

    pub engine                  : Engine,

    // All instances
    pub instances               : Vec<BehaviorInstance>,
    // Currently active instances
    pub active_instance_indices : Vec<usize>,

    /// The behavior id of the "Player" character, cached for faster instancing
    pub player_behavior_id      : usize,

    pub player_ids_inst_indices : HashMap<usize, usize>,

    // Current region id and index
    pub curr_region_id          : usize,

    // Script scopes
    pub scopes                  : Vec<Scope<'a>>,
    // Script ast's, id is (BehaviorType, BehaviorId, BehaviorNodeID, AtomParameterID)
    pub ast                     : HashMap<(BehaviorType, usize, usize, String), AST>,

    // Custom scopes (mostly used in Widgets to store the Widgets rect)
    pub custom_scopes           : HashMap<usize, Scope<'a>>,
    pub custom_scopes_ordered   : Vec<usize>,

    pub runs_in_editor          : bool,

    pub characters              : HashMap<usize, Vec<CharacterData>>,

    // These are fields which provide feedback to the editor / game while running
    pub messages                : Vec<(String, MessageType)>,
    pub executed_connections    : Vec<(BehaviorType, usize, BehaviorNodeConnector)>,
    pub changed_variables       : Vec<(usize, usize, usize, f64)>, // A variable has been changed: instance index, behavior id, node id, new value
}

impl GameData<'_> {

    // Load the game data from the given path
    pub fn load_from_path(path: path::PathBuf) -> Self {

        // Create the tile regions
        let mut regions: HashMap<usize, GameRegion> = HashMap::new();
        let mut regions_names = vec![];
        let mut regions_ids = vec![];

        let region_path = path.join("game").join("regions");

        let mut paths: Vec<_> = fs::read_dir(region_path.clone()).unwrap()
                                                .map(|r| r.unwrap())
                                                .collect();
        paths.sort_by_key(|dir| dir.path());

        for path in paths {
            let path = &path.path();
            let md = metadata(path).unwrap();

            if md.is_dir() {
                let mut region = GameRegion::new(path, &region_path);
                regions_names.push(region.name.clone());

                // Make sure we create a unique id (check if the id already exists in the set)
                let mut has_id_already = true;
                while has_id_already {

                    has_id_already = false;
                    for (key, _value) in &regions {
                        if key == &region.data.id {
                            has_id_already = true;
                        }
                    }

                    if has_id_already {
                        region.data.id += 1;
                    }
                }

                region.calc_dimensions();

                regions_ids.push(region.data.id);
                regions.insert(region.data.id, region);
            }
        }

        let sorted_keys= regions.keys().sorted();
        for key in sorted_keys {
            let region = &regions[key];

            // If the region has no tiles we assume it's new and we save the data
            if region.data.layer1.len() == 0 {
                region.save_data();
            }
        }

        // Behaviors

        let mut behaviors: HashMap<usize, GameBehavior> = HashMap::new();
        let mut behaviors_names = vec![];
        let mut behaviors_ids = vec![];

        let behavior_path = path.join("game").join("characters");
        if let Some(paths) = fs::read_dir(behavior_path.clone()).ok() {

            for path in paths {
                let path = &path.unwrap().path();
                let md = metadata(path).unwrap();

                if md.is_file() {
                    if let Some(name) = path::Path::new(&path).extension() {
                        if name == "json" || name == "JSON" {
                            let mut behavior = GameBehavior::load_from_path(path, &behavior_path);
                            behaviors_names.push(behavior.name.clone());

                            // Make sure we create a unique id (check if the id already exists in the set)
                            let mut has_id_already = true;
                            while has_id_already {

                                has_id_already = false;
                                for (key, _value) in &behaviors {
                                    if key == &behavior.data.id {
                                        has_id_already = true;
                                    }
                                }

                                if has_id_already {
                                    behavior.data.id += 1;
                                }
                            }

                            if behavior.data.nodes.len() == 0 {
                                behavior.add_node(BehaviorNodeType::BehaviorType, "Behavior Type".to_string());
                                behavior.add_node(BehaviorNodeType::BehaviorTree, "Behavior Tree".to_string());
                                behavior.save_data();
                            }
                            behaviors_ids.push(behavior.data.id);
                            behaviors.insert(behavior.data.id, behavior);
                        }
                    }
                }
            }
        }

        // Make sure the Player character is always first in the list
        let mut player_index : Option<usize> = None;
        for (index, b) in behaviors_names.iter().enumerate() {
            if b == "Player" {
                player_index = Some(index);
            }
        }

        if let Some(player_index) = player_index {
            let name = behaviors_names.remove(player_index);
            let id = behaviors_ids.remove(player_index);
            behaviors_names.insert(0, name);
            behaviors_ids.insert(0, id);
        }

        // Systems

        let mut systems: HashMap<usize, GameBehavior> = HashMap::new();
        let mut systems_names = vec![];
        let mut systems_ids = vec![];

        let systems_path = path.join("game").join("systems");
        if let Some(paths) = fs::read_dir(systems_path.clone()).ok() {

            for path in paths {
                let path = &path.unwrap().path();
                let md = metadata(path).unwrap();

                if md.is_file() {
                    if let Some(name) = path::Path::new(&path).extension() {
                        if name == "json" || name == "JSON" {
                            let mut system = GameBehavior::load_from_path(path, &systems_path);
                            systems_names.push(system.name.clone());

                            // Make sure we create a unique id (check if the id already exists in the set)
                            let mut has_id_already = true;
                            while has_id_already {

                                has_id_already = false;
                                for (key, _value) in &systems {
                                    if key == &system.data.id {
                                        has_id_already = true;
                                    }
                                }

                                if has_id_already {
                                    system.data.id += 1;
                                }
                            }

                            if system.data.nodes.len() == 0 {
                                // behavior.add_node(BehaviorNodeType::BehaviorType, "Behavior Type".to_string());
                                // behavior.add_node(BehaviorNodeType::BehaviorTree, "Behavior Tree".to_string());
                                // behavior.save_data();
                            }
                            systems_ids.push(system.data.id);
                            systems.insert(system.data.id, system);
                        }
                    }
                }
            }
        }

        // Items

        let mut items: HashMap<usize, GameBehavior> = HashMap::new();
        let mut items_names = vec![];
        let mut items_ids = vec![];

        let item_path = path.join("game").join("items");
        if let Some(paths) = fs::read_dir(item_path.clone()).ok() {

            for path in paths {
                let path = &path.unwrap().path();
                let md = metadata(path).unwrap();

                if md.is_file() {
                    if let Some(name) = path::Path::new(&path).extension() {
                        if name == "json" || name == "JSON" {
                            let mut item = GameBehavior::load_from_path(path, &item_path);
                            items_names.push(item.name.clone());

                            // Make sure we create a unique id (check if the id already exists in the set)
                            let mut has_id_already = true;
                            while has_id_already {

                                has_id_already = false;
                                for (key, _value) in &behaviors {
                                    if key == &item.data.id {
                                        has_id_already = true;
                                    }
                                }

                                if has_id_already {
                                    item.data.id += 1;
                                }
                            }

                            if item.data.nodes.len() == 0 {
                                // behavior.add_node(BehaviorNodeType::BehaviorType, "Behavior Type".to_string());
                                // behavior.add_node(BehaviorNodeType::BehaviorTree, "Behavior Tree".to_string());
                                // behavior.save_data();
                            }
                            items_ids.push(item.data.id);
                            items.insert(item.data.id, item);
                        }
                    }
                }
            }
        }

        // Game

        let mut game = Game::load_from_path(&path.clone());
        if game.behavior.data.nodes.is_empty() {

            game.behavior.add_node(BehaviorNodeType::BehaviorType, "Behavior Type".to_string());
            game.behavior.add_node(BehaviorNodeType::BehaviorTree, "Game".to_string());

            game.save_data();
        }

        let mut nodes : HashMap<BehaviorNodeType, NodeCall> = HashMap::new();
        nodes.insert(BehaviorNodeType::Expression, nodes::expression);
        nodes.insert(BehaviorNodeType::Script, nodes::script);
        nodes.insert(BehaviorNodeType::Message, nodes::message);
        nodes.insert(BehaviorNodeType::Pathfinder, nodes::pathfinder);
        nodes.insert(BehaviorNodeType::Lookout, nodes::lookout);
        nodes.insert(BehaviorNodeType::CloseIn, nodes::close_in);
        nodes.insert(BehaviorNodeType::CallSystem, nodes::call_system);
        nodes.insert(BehaviorNodeType::CallBehavior, nodes::call_behavior);
        nodes.insert(BehaviorNodeType::LockTree, nodes::lock_tree);
        nodes.insert(BehaviorNodeType::UnlockTree, nodes::unlock_tree);
        nodes.insert(BehaviorNodeType::SetState, nodes::set_state);

        nodes.insert(BehaviorNodeType::InsideArea, nodes_area::inside_area);
        nodes.insert(BehaviorNodeType::DisplaceTiles, nodes_area::displace_tiles);

        nodes.insert(BehaviorNodeType::Move, nodes::player_move);

        nodes.insert(BehaviorNodeType::Screen, nodes_game::screen);
        nodes.insert(BehaviorNodeType::Widget, nodes_game::widget);
        nodes.insert(BehaviorNodeType::Settings, nodes_game::settings);

        let mut engine = Engine::new();

        // Variable resolver for d??? -> random(???)
        #[allow(deprecated)]
        engine.on_var(|name, _index, _context| {

            if name.starts_with("d") {
                let mut s = name.to_string();
                s.remove(0);
                if let Some(n) = s.parse::<i64>().ok() {
                    let mut rng = thread_rng();
                    let random = rng.gen_range(1..=n) as f64;
                    //println!{"d{} {}",n, random};
                    return Ok(Some(random.into()));
                }
            }
            Ok(None)
        });

        // Display f64 as ints
        use pathfinding::num_traits::ToPrimitive;
        engine.register_fn("to_string", |x: f64| format!("{}", x.to_isize().unwrap()));

        Self {

            path                    : path.clone(),
            asset                   : None,
            draw2d                  : None,

            regions,
            regions_names,
            regions_ids,

            behaviors,
            behaviors_names,
            behaviors_ids,

            systems,
            systems_names,
            systems_ids,

            items,
            items_names,
            items_ids,

            game,
            game_instance_index     : None,

            game_screen_width       : 1024,
            game_screen_height      : 608,
            game_screen_tile_size   : 32,

            game_screens            : HashMap::new(),

            game_anim_counter       : 0,
            game_frame              : vec![],

            nodes,

            engine,

            instances               : vec![],
            active_instance_indices : vec![],

            player_behavior_id      : 0,
            player_ids_inst_indices : HashMap::new(),

            curr_region_id          : 0,

            scopes                  : vec![],
            ast                     : HashMap::new(),

            custom_scopes           : HashMap::new(),
            custom_scopes_ordered   : vec![],

            runs_in_editor          : false,

            characters              : HashMap::new(),

            messages                : vec![],
            executed_connections    : vec![],
            changed_variables       : vec![],
        }
    }

    // Create an empty structure
    pub fn new() -> Self {

        let regions: HashMap<usize, GameRegion> = HashMap::new();
        let regions_names = vec![];
        let regions_ids = vec![];

        // Behaviors

        let behaviors: HashMap<usize, GameBehavior> = HashMap::new();
        let behaviors_names = vec![];
        let behaviors_ids = vec![];

        // Systems

        let systems: HashMap<usize, GameBehavior> = HashMap::new();
        let systems_names = vec![];
        let systems_ids = vec![];

        // Items

        let items: HashMap<usize, GameBehavior> = HashMap::new();
        let items_names = vec![];
        let items_ids = vec![];

        // Game

        let game = Game::new();

        //
        let nodes : HashMap<BehaviorNodeType, NodeCall> = HashMap::new();
        let engine = Engine::new();

        Self {

            path                    : PathBuf::new(),
            asset                   : None,
            draw2d                  : None,

            regions,
            regions_names,
            regions_ids,

            behaviors,
            behaviors_names,
            behaviors_ids,

            systems,
            systems_names,
            systems_ids,

            items,
            items_names,
            items_ids,

            game,
            game_instance_index     : None,

            game_screen_width       : 1024,
            game_screen_height      : 608,
            game_screen_tile_size   : 32,

            game_screens            : HashMap::new(),

            game_anim_counter       : 0,
            game_frame              : vec![],

            nodes,

            engine,

            instances               : vec![],
            active_instance_indices : vec![],

            player_behavior_id      : 0,
            player_ids_inst_indices : HashMap::new(),

            curr_region_id          : 0,

            scopes                  : vec![],
            ast                     : HashMap::new(),

            custom_scopes           : HashMap::new(),
            custom_scopes_ordered   : vec![],

            runs_in_editor          : false,

            characters              : HashMap::new(),

            messages                : vec![],
            executed_connections    : vec![],
            changed_variables       : vec![],
        }
    }

    #[cfg(feature = "data_editing")]
    /// Saves the region to disk
    pub fn save_region(&self, id: usize) {
        if let Some(region) = &mut self.regions.get(&id) {
            region.save_data();
        }
    }

    #[cfg(feature = "data_editing")]
    /// Sets a value in the region
    pub fn set_region_value(&mut self, layer: usize, id: usize, pos: (isize, isize), value: (usize, usize, usize, TileUsage)) {
        let region = &mut self.regions.get_mut(&id).unwrap();
        region.set_value(layer, pos, value);
    }

    #[cfg(feature = "data_editing")]
    /// Get region by name
    pub fn get_region_by_name(&self, name: &String) -> Option<&GameRegion> {

        for (index, n) in self.regions_names.iter().enumerate() {
            if n == name {
                return self.regions.get(&index);
            }
        }
        None
    }

    /// Create a new behavior
    pub fn create_behavior(&mut self, name: String, _behavior_type: usize) {

        let path = self.path.join("game").join("behavior").join(name.clone() + ".json");

        let mut behavior = GameBehavior::load_from_path(&path, &self.path.join("game").join("behavior"));
        behavior.data.name = name.clone();

        self.behaviors_names.push(behavior.name.clone());
        self.behaviors_ids.push(behavior.data.id);

        behavior.add_node(BehaviorNodeType::BehaviorType, "Behavior Type".to_string());
        behavior.add_node(BehaviorNodeType::BehaviorTree, "Behavior Tree".to_string());
        behavior.save_data();

        self.behaviors.insert(behavior.data.id, behavior);
    }

    /// Create a new behavior
    pub fn create_region(&mut self, name: String) -> bool {
        let path = self.path.join("game").join("regions").join(name.clone());

        if fs::create_dir(path.clone()).ok().is_some() {
            let region = GameRegion::new(&path, &self.path.join("game").join("regions"));

            self.regions_names.push(region.name.clone());
            self.regions_ids.push(region.data.id);

            region.save_data();

            self.regions.insert(region.data.id, region);

            return true;
        }

        false
    }


    /// Create a new system
    pub fn create_system(&mut self, name: String, _behavior_type: usize) {

        let path = self.path.join("game").join("systems").join(name.clone() + ".json");

        let mut system = GameBehavior::load_from_path(&path, &self.path.join("game").join("systems"));
        system.data.name = name.clone();

        self.systems_names.push(system.name.clone());
        self.systems_ids.push(system.data.id);

        system.add_node(BehaviorNodeType::BehaviorTree, "Behavior Tree".to_string());
        system.save_data();

        self.systems.insert(system.data.id, system);
    }

    #[cfg(feature = "data_editing")]
    /// Sets the value for the given behavior id
    pub fn set_behavior_id_value(&mut self, id: (usize, usize, String), value: (f64, f64, f64, f64, String), behavior_type: BehaviorType) {
        if let Some(behavior) = self.get_mut_behavior(id.0, behavior_type) {
            if let Some(node) = behavior.data.nodes.get_mut(&id.1) {
                node.values.insert(id.2.clone(), value);
                behavior.save_data();
            }
        }
    }

    #[cfg(feature = "data_editing")]
    /// Sets the name for the given node
    pub fn set_behavior_node_name(&mut self, id: (usize, usize), value: String, behavior_type: BehaviorType) {
        if let Some(behavior) = self.get_mut_behavior(id.0, behavior_type) {
            if let Some(node) = behavior.data.nodes.get_mut(&id.1) {
                node.name = value;
                behavior.save_data();
            }
        }
    }

    #[cfg(feature = "data_editing")]
    /// Gets the value of the behavior id
    pub fn get_behavior_id_value(&self, id: (usize, usize, String), def: (f64, f64, f64, f64, String), behavior_type: BehaviorType) -> (f64, f64, f64, f64, String) {
        if let Some(behavior) = self.get_behavior(id.0, behavior_type) {
            if let Some(node) = behavior.data.nodes.get(&id.1) {
                if let Some(v) = node.values.get(&id.2) {
                    return v.clone();
                }
            }
        }
        def
    }

    #[cfg(feature = "data_editing")]
    /// Gets the position for the given behavior
    pub fn get_behavior_default_position(&self, id: usize) -> Option<(usize, isize, isize)> {
        if let Some(behavior) = self.behaviors.get(&id) {
            for (_index, node) in &behavior.data.nodes {
                if node.behavior_type == BehaviorNodeType::BehaviorType {
                    if let Some(position) = node.values.get(&"position".to_string()) {
                        return Some((position.0 as usize, position.1 as isize, position.2 as isize));
                    }
                }
            }
        }
        None
    }

    #[cfg(feature = "data_editing")]
    /// Gets the position for the given behavior
    pub fn get_behavior_default_tile(&self, id: usize) -> Option<(usize, usize, usize)> {
        if let Some(behavior) = self.behaviors.get(&id) {
            for (_index, node) in &behavior.data.nodes {
                if node.behavior_type == BehaviorNodeType::BehaviorType {
                    if let Some(tile) = node.values.get(&"tile".to_string()) {
                        return Some((tile.0 as usize, tile.1 as usize, tile.2 as usize));
                    }
                }
            }
        }
        None
    }

    /// Save data and return it
    pub fn save(&self) -> String {
        let json = serde_json::to_string(&self.instances).unwrap();
        json
    }

    /// Create a new behavior instance for the given id and return it's instance index
    pub fn create_behavior_instance(&mut self, id: usize) -> usize {

        let mut to_execute : Vec<usize> = vec![];

        let mut position : Option<(usize, isize, isize)> = None;
        let mut tile     : Option<(usize, usize, usize)> = None;

        let mut scope = Scope::new();

        // Insert Dices
        /*
        for d in (2..=20).step_by(2) {
            scope.push( format!("d{}", d), 0.0 as f64);
        }
        scope.push( "d100", 0.0 as f64);*/

        // Default values
        scope.push("Value1", 0.0_f64);
        scope.push("Value2", 0.0_f64);
        scope.push("Value3", 0.0_f64);

        if let Some(behavior) = self.behaviors.get_mut(&id) {
            for (id, node) in &behavior.data.nodes {
                if node.behavior_type == BehaviorNodeType::BehaviorTree {

                    for c in &behavior.data.connections {
                        if c.0 == *id {
                            to_execute.push(c.0);
                        }
                    }
                } else
                if node.behavior_type == BehaviorNodeType::BehaviorType {
                    if let Some(value )= node.values.get(&"position".to_string()) {
                        position = Some((value.0 as usize, value.1 as isize, value.2 as isize));
                    }
                    if let Some(value )= node.values.get(&"tile".to_string()) {
                        tile = Some((value.0 as usize, value.1 as usize, value.2 as usize));
                    }
                } else
                if node.behavior_type == BehaviorNodeType::VariableNumber {
                    if let Some(value )= node.values.get(&"value".to_string()) {
                        scope.push(node.name.clone(), value.0.clone());
                    } else {
                        scope.push(node.name.clone(), 0.0_f64);
                    }
                }
            }

            let index = self.instances.len();

            let mut instance = BehaviorInstance {id: thread_rng().gen_range(1..=u32::MAX) as usize, state: BehaviorInstanceState::Normal, name: behavior.name.clone(), behavior_id: id, tree_ids: to_execute.clone(), position, tile, target_instance_index: None, locked_tree: None, party: vec![], node_values: HashMap::new(), state_values: HashMap::new(), number_values: HashMap::new(), sleep_cycles: 0, systems_id: 0, action: None, instance_type: behavior::BehaviorInstanceType::NonPlayerCharacter, update: None, regions_send: HashSet::new()};

            // Make sure id is unique
            let mut has_id_already = true;
            while has_id_already {
                has_id_already = false;
                for index in 0..self.instances.len() {
                    if self.instances[index].id == instance.id {
                        has_id_already = true;
                    }
                }

                if has_id_already {
                    instance.id += 1;
                }
            }

            self.instances.push(instance);
            self.scopes.push(scope);

            return index;
        }

        0
    }

    /// Create the game instance and return it's instance index
    pub fn create_game_instance(&mut self) -> usize {

        let mut to_execute : Vec<usize> = vec![];

        let mut startup_name : Option<String> = None;
        let mut locked_tree  : Option<usize> = None;

        let mut scope = Scope::new();

        let behavior = &mut self.game.behavior;

        // Collect name of the startup tree and the variables
        for (_id, node) in &behavior.data.nodes {
            if node.behavior_type == BehaviorNodeType::BehaviorType {
                if let Some(value )= node.values.get(&"startup".to_string()) {
                    startup_name = Some(value.4.clone());
                }
            } else
            if node.behavior_type == BehaviorNodeType::VariableNumber {
                if let Some(value )= node.values.get(&"value".to_string()) {
                    scope.push(node.name.clone(), value.0.clone());
                } else {
                    scope.push(node.name.clone(), 0.0_f64);
                }
            }
        }

        // Second pass parse the trees and find the startup tree
        for (id, node) in &behavior.data.nodes {
            if node.behavior_type == BehaviorNodeType::BehaviorTree {

                for c in &behavior.data.connections {
                    if c.0 == *id {
                        to_execute.push(c.0);
                        if let Some(startup) = startup_name.clone() {
                            if node.name == startup {
                                locked_tree = Some(node.id);
                            }
                        }
                    }
                }
            }
        }

        let index = self.instances.len();

        let mut instance = BehaviorInstance {id: thread_rng().gen_range(1..=u32::MAX) as usize, state: BehaviorInstanceState::Normal, name: behavior.name.clone(), behavior_id: behavior.data.id, tree_ids: to_execute.clone(), position: None, tile: None, target_instance_index: None, locked_tree, party: vec![], node_values: HashMap::new(), state_values: HashMap::new(), number_values: HashMap::new(), sleep_cycles: 0, systems_id: 0, action: None, instance_type: behavior::BehaviorInstanceType::GameLogic, update: None, regions_send: HashSet::new()};

        // Make sure id is unique
        let mut has_id_already = true;
        while has_id_already {
            has_id_already = false;
            for index in 0..self.instances.len() {
                if self.instances[index].id == instance.id {
                    has_id_already = true;
                }
            }

            if has_id_already {
                instance.id += 1;
            }
        }

        self.instances.push(instance);
        self.scopes.push(scope);

        for tree_id in &to_execute {
            // Execute this tree if it is a "Startup" Only tree

            if let Some(value)= get_node_value((self.instances[index].behavior_id, *tree_id, "execute"), self, BehaviorType::GameLogic) {
                if value.0 == 1.0 {
                    self.execute_game_node(index, tree_id.clone());
                }
            }
        }

        return index;
    }

    /// Returns the layered tiles at the given position
    pub fn get_tile_at(&self, pos: (usize, isize, isize)) -> Vec<(usize, usize, usize, TileUsage)> {
        if let Some(region) = self.regions.get(&pos.0) {
            return region.get_value((pos.1, pos.2));
        }
        vec![]
    }

    /// Delete the region of the given id
    pub fn delete_region(&mut self, index: &usize) {
        let id = self.regions_ids[*index].clone();

        if let Some(region) = self.regions.get(&id) {
            let _ = std::fs::remove_dir_all(region.path.clone());
        }

        self.regions_names.remove(*index);
        self.regions_ids.remove(*index);
        self.regions.remove(&id);
    }

    /// Delete the behavior of the given id
    pub fn delete_behavior(&mut self, index: &usize) {
        let id = self.behaviors_ids[*index].clone();

        if let Some(behavior) = self.behaviors.get(&id) {
            let _ = std::fs::remove_file(behavior.path.clone());
        }

        self.behaviors_names.remove(*index);
        self.behaviors_ids.remove(*index);
        self.behaviors.remove(&id);
    }

    /// Delete the system of the given id
    pub fn delete_system(&mut self, index: &usize) {
        let id = self.systems_ids[*index].clone();

        if let Some(system) = self.systems.get(&id) {
            let _ = std::fs::remove_file(system.path.clone());
        }

        self.systems_names.remove(*index);
        self.systems_ids.remove(*index);
        self.systems.remove(&id);
    }

    /// Executes the given node and follows the connection chain
    fn execute_node(&mut self, instance_index: usize, node_id: usize) -> Option<BehaviorNodeConnector> {

        let mut connectors : Vec<BehaviorNodeConnector> = vec![];
        let mut connected_node_ids : Vec<usize> = vec![];
        let mut possibly_executed_connections : Vec<(BehaviorType, usize, BehaviorNodeConnector)> = vec![];

        let mut is_sequence = false;
        let mut rc : Option<BehaviorNodeConnector> = None;

        // Call the node and get the resulting BehaviorNodeConnector
        if let Some(behavior) = self.behaviors.get_mut(&self.instances[instance_index].behavior_id) {
            if let Some(node) = behavior.data.nodes.get_mut(&node_id) {

                // Handle special nodes
                if node.behavior_type == BehaviorNodeType::BehaviorTree || node.behavior_type == BehaviorNodeType::Linear {
                    connectors.push(BehaviorNodeConnector::Bottom1);
                    connectors.push(BehaviorNodeConnector::Bottom2);
                    connectors.push(BehaviorNodeConnector::Bottom);
                    connectors.push(BehaviorNodeConnector::Bottom3);
                    connectors.push(BehaviorNodeConnector::Bottom4);
                } else
                if node.behavior_type == BehaviorNodeType::Sequence {
                    connectors.push(BehaviorNodeConnector::Bottom1);
                    connectors.push(BehaviorNodeConnector::Bottom2);
                    connectors.push(BehaviorNodeConnector::Bottom);
                    connectors.push(BehaviorNodeConnector::Bottom3);
                    connectors.push(BehaviorNodeConnector::Bottom4);
                    is_sequence = true;
                } else {
                    if let Some(node_call) = self.nodes.get_mut(&node.behavior_type) {
                        let behavior_id = self.instances[instance_index].behavior_id.clone();
                        let connector = node_call(instance_index, (behavior_id, node_id), self, BehaviorType::Behaviors);
                        rc = Some(connector);
                        connectors.push(connector);
                    } else {
                        connectors.push(BehaviorNodeConnector::Bottom);
                    }
                }
            }
        }

        // Search the connections to check if we can find an ongoing node connection
        for connector in connectors {
            if let Some(behavior) = self.behaviors.get_mut(&self.instances[instance_index].behavior_id) {

                for c in &behavior.data.connections {
                    if c.0 == node_id && c.1 == connector {
                        connected_node_ids.push(c.2);
                        if is_sequence == false {
                            self.executed_connections.push((BehaviorType::Behaviors, c.0, c.1));
                        } else {
                            possibly_executed_connections.push((BehaviorType::Behaviors, c.0, c.1));
                        }
                    }
                }
            }
        }

        // And if yes execute it
        for (index, connected_node_id) in connected_node_ids.iter().enumerate() {

            // If this is a sequence, mark this connection as executed
            if is_sequence {
                self.executed_connections.push(possibly_executed_connections[index]);
            }

            if let Some(connector) = self.execute_node(instance_index, *connected_node_id) {
                if is_sequence {
                    // Inside a sequence break out if the connector is not Success
                    if connector == BehaviorNodeConnector::Fail || connector == BehaviorNodeConnector::Right {
                        break;
                    }
                }
            }
        }
        rc
    }

    /// Executes the given systems node and follows the connection chain
    fn execute_systems_node(&mut self, instance_index: usize, node_id: usize) -> Option<BehaviorNodeConnector> {

        let mut connectors : Vec<BehaviorNodeConnector> = vec![];
        let mut connected_node_ids : Vec<usize> = vec![];
        let mut possibly_executed_connections : Vec<(BehaviorType, usize, BehaviorNodeConnector)> = vec![];

        let mut is_sequence = false;
        let mut rc : Option<BehaviorNodeConnector> = None;

        // Call the node and get the resulting BehaviorNodeConnector
        if let Some(system) = self.systems.get_mut(&self.instances[instance_index].systems_id) {
            if let Some(node) = system.data.nodes.get_mut(&node_id) {

                // Handle special nodes
                if node.behavior_type == BehaviorNodeType::BehaviorTree || node.behavior_type == BehaviorNodeType::Linear {
                    connectors.push(BehaviorNodeConnector::Bottom1);
                    connectors.push(BehaviorNodeConnector::Bottom2);
                    connectors.push(BehaviorNodeConnector::Bottom);
                } else
                if node.behavior_type == BehaviorNodeType::Sequence {
                    connectors.push(BehaviorNodeConnector::Bottom1);
                    connectors.push(BehaviorNodeConnector::Bottom2);
                    connectors.push(BehaviorNodeConnector::Bottom);
                    connectors.push(BehaviorNodeConnector::Bottom3);
                    connectors.push(BehaviorNodeConnector::Bottom4);                    is_sequence = true;
                } else {
                    if let Some(node_call) = self.nodes.get_mut(&node.behavior_type) {
                        let systems_id = self.instances[instance_index].systems_id.clone();
                        let connector = node_call(instance_index, (systems_id, node_id), self, BehaviorType::Systems);
                        rc = Some(connector);
                        connectors.push(connector);
                    } else {
                        connectors.push(BehaviorNodeConnector::Bottom);
                    }
                }
            }
        }

        // Search the connections to check if we can find an ongoing node connection
        for connector in connectors {
            if let Some(system) = self.systems.get_mut(&self.instances[instance_index].systems_id) {

                for c in &system.data.connections {
                    if c.0 == node_id && c.1 == connector {
                        connected_node_ids.push(c.2);
                        if is_sequence == false {
                            self.executed_connections.push((BehaviorType::Systems, c.0, c.1));
                        } else {
                            possibly_executed_connections.push((BehaviorType::Systems, c.0, c.1));
                        }
                    }
                }
            }
        }

        // And if yes execute it
        for (index, connected_node_id) in connected_node_ids.iter().enumerate() {

            // If this is a sequence, mark this connection as executed
            if is_sequence {
                self.executed_connections.push(possibly_executed_connections[index]);
            }

            if let Some(connector) = self.execute_systems_node(instance_index, *connected_node_id) {
                if is_sequence {
                    // Inside a sequence break out if the connector is not Success
                    if connector == BehaviorNodeConnector::Fail || connector == BehaviorNodeConnector::Right {
                        break;
                    }
                }
            }
        }
        rc
    }

    /// Executes the given node and follows the connection chain
    fn execute_area_node(&mut self, area_index: usize, node_id: usize) -> Option<BehaviorNodeConnector> {

        let mut connectors : Vec<BehaviorNodeConnector> = vec![];
        let mut connected_node_ids : Vec<usize> = vec![];

        let mut rc : Option<BehaviorNodeConnector> = None;

        // Call the node and get the resulting BehaviorNodeConnector
        if let Some(region) = self.regions.get_mut(&self.curr_region_id) {
            if let Some(node) = region.behaviors[area_index].data.nodes.get_mut(&node_id) {

                if let Some(node_call) = self.nodes.get_mut(&node.behavior_type) {
                    let connector = node_call(region.behaviors[area_index].data.id, (area_index, node_id), self, BehaviorType::Regions);
                    rc = Some(connector);
                    connectors.push(connector);
                } else {
                    connectors.push(BehaviorNodeConnector::Bottom);
                }
            }
        }

        // Search the connections to check if we can find an ongoing node connection
        for connector in connectors {
            if let Some(region) = self.regions.get_mut(&self.curr_region_id) {
                for c in &region.behaviors[area_index].data.connections {
                    if c.0 == node_id && c.1 == connector {
                        connected_node_ids.push(c.2);
                        self.executed_connections.push((BehaviorType::Regions, c.0, c.1));
                    }
                }
            }
        }

        // And if yes execute it
        for (_index, connected_node_id) in connected_node_ids.iter().enumerate() {
            self.execute_area_node(area_index, *connected_node_id);
        }
        rc
    }

    /// Executes the given node and follows the connection chain
    fn execute_game_node(&mut self, instance_index: usize, node_id: usize) -> Option<BehaviorNodeConnector> {

        let mut connectors : Vec<BehaviorNodeConnector> = vec![];
        let mut connected_node_ids : Vec<usize> = vec![];
        let mut possibly_executed_connections : Vec<(BehaviorType, usize, BehaviorNodeConnector)> = vec![];

        let mut is_sequence = false;
        let mut rc : Option<BehaviorNodeConnector> = None;

        // Call the node and get the resulting BehaviorNodeConnector
        let behavior = &mut self.game.behavior;
        if let Some(node) = behavior.data.nodes.get_mut(&node_id) {

            // Handle special nodes
            if node.behavior_type == BehaviorNodeType::Screen{
                connectors.push(BehaviorNodeConnector::Bottom1);
                connectors.push(BehaviorNodeConnector::Bottom2);
                connectors.push(BehaviorNodeConnector::Bottom);
                connectors.push(BehaviorNodeConnector::Bottom3);
                connectors.push(BehaviorNodeConnector::Bottom4);

                if let Some(node_call) = self.nodes.get_mut(&node.behavior_type) {
                    let behavior_id = self.instances[instance_index].behavior_id.clone();
                    _ = node_call(instance_index, (behavior_id, node_id), self, BehaviorType::GameLogic);
                }
            } else
            if node.behavior_type == BehaviorNodeType::BehaviorTree || node.behavior_type == BehaviorNodeType::Linear {
                connectors.push(BehaviorNodeConnector::Bottom1);
                connectors.push(BehaviorNodeConnector::Bottom2);
                connectors.push(BehaviorNodeConnector::Bottom);
                connectors.push(BehaviorNodeConnector::Bottom3);
                connectors.push(BehaviorNodeConnector::Bottom4);
            } else
            if node.behavior_type == BehaviorNodeType::Sequence {
                connectors.push(BehaviorNodeConnector::Bottom1);
                connectors.push(BehaviorNodeConnector::Bottom2);
                connectors.push(BehaviorNodeConnector::Bottom);
                connectors.push(BehaviorNodeConnector::Bottom3);
                connectors.push(BehaviorNodeConnector::Bottom4);
                is_sequence = true;
            } else {
                if let Some(node_call) = self.nodes.get_mut(&node.behavior_type) {
                    let behavior_id = self.instances[instance_index].behavior_id.clone();
                    let connector = node_call(instance_index, (behavior_id, node_id), self, BehaviorType::GameLogic);
                    rc = Some(connector);
                    connectors.push(connector);
                } else {
                    connectors.push(BehaviorNodeConnector::Bottom);
                }
            }
        }

        // Search the connections to check if we can find an ongoing node connection
        for connector in connectors {
            let behavior = &mut self.game.behavior;
            for c in &behavior.data.connections {
                if c.0 == node_id && c.1 == connector {
                    connected_node_ids.push(c.2);
                    if is_sequence == false {
                        self.executed_connections.push((BehaviorType::GameLogic, c.0, c.1));
                    } else {
                        possibly_executed_connections.push((BehaviorType::GameLogic, c.0, c.1));
                    }
                }
            }
        }

        // And if yes execute it
        for (index, connected_node_id) in connected_node_ids.iter().enumerate() {

            // If this is a sequence, mark this connection as executed
            if is_sequence {
                self.executed_connections.push(possibly_executed_connections[index]);
            }

            if let Some(connector) = self.execute_game_node(instance_index, *connected_node_id) {
                if is_sequence {
                    // Inside a sequence break out if the connector is not Success
                    if connector == BehaviorNodeConnector::Fail || connector == BehaviorNodeConnector::Right {
                        break;
                    }
                }
            }
        }
        rc
    }


    /// Gets the behavior for the given behavior type
    pub fn get_behavior(&self, id: usize, behavior_type: BehaviorType) -> Option<&GameBehavior> {
        if behavior_type == BehaviorType::Regions {
            for (_index, region) in &self.regions {
                for index in 0..region.behaviors.len() {
                    if region.behaviors[index].data.id == id {
                        return Some(&region.behaviors[index]);
                    }
                }
            }
        } else
        if behavior_type == BehaviorType::Behaviors {
            return self.behaviors.get(&id);
        } else
        if behavior_type == BehaviorType::Systems {
            return self.systems.get(&id);
        } else
        if behavior_type == BehaviorType::Items {
            return self.items.get(&id);
        } else
        if behavior_type == BehaviorType::GameLogic {
            return Some(&self.game.behavior);
        }
        None
    }

    /// Gets the mutable behavior for the given behavior type
    pub fn get_mut_behavior(&mut self, id: usize, behavior_type: BehaviorType) -> Option<&mut GameBehavior> {
        if behavior_type == BehaviorType::Regions {
            for (_index, region) in &mut self.regions {
                for index in 0..region.behaviors.len() {
                    if region.behaviors[index].data.id == id {
                        return Some(&mut region.behaviors[index]);
                    }
                }
            }
        } else
        if behavior_type == BehaviorType::Behaviors {
            return self.behaviors.get_mut(&id);
        } else
        if behavior_type == BehaviorType::Systems {
            return self.systems.get_mut(&id);
        } else
        if behavior_type == BehaviorType::Items {
            return self.items.get_mut(&id);
        } else
        if behavior_type == BehaviorType::GameLogic {
            return Some(&mut self.game.behavior);
        }
        None
    }

    // Instance Handling

    /// Creates all behavior instances
    pub fn create_behavior_instances(&mut self) {
        self.active_instance_indices = vec![];
        for index in 0..self.behaviors_ids.len() {
            if self.behaviors_names[index] != "Player" {
                self.create_behavior_instance(self.behaviors_ids[index]);
            } else {
                self.player_behavior_id = self.behaviors_ids[index];
            }
        }
    }

    /// Activate the instances for the given region, making this region the current one
    pub fn activate_region_instances(&mut self, region_id: usize) {
        self.active_instance_indices = vec![];
        self.curr_region_id = region_id;

        for index in 0..self.instances.len() {
            if let Some(position) = self.instances[index].position {
                if position.0 == region_id {
                    self.active_instance_indices.push(index);
                }
            }
        }
    }

    /// Game tick
    pub fn tick(&mut self, size: Option<(usize, usize, usize)>) {
        self.executed_connections = vec![];
        self.changed_variables = vec![];
        self.characters = HashMap::new();

        // Execute behaviors
        for index in 0..self.active_instance_indices.len() {
            let inst_index = self.active_instance_indices[index];

            // Skip Sleep cycles
            if self.instances[inst_index].sleep_cycles > 0 {
                self.instances[inst_index].sleep_cycles -= 1;
            } else {

                // Killed or Purged: Skip
                if self.instances[inst_index].state == BehaviorInstanceState::Purged || self.instances[inst_index].state == BehaviorInstanceState::Killed {
                    continue;
                }

                if self.instances[inst_index].instance_type == BehaviorInstanceType::NonPlayerCharacter {
                    // Execute trees of an NPC

                    // Has a locked tree ?
                    if let Some(locked_tree) = self.instances[inst_index].locked_tree {
                            self.execute_node(inst_index, locked_tree);
                    } else {
                        // Unlocked, execute all valid trees
                        let trees = self.instances[inst_index].tree_ids.clone();
                        for node_id in &trees {

                            // Only execute trees here with an "Always" execute setting (0)
                            if let Some(value)= get_node_value((self.instances[inst_index].behavior_id, *node_id, "execute"), self, BehaviorType::Behaviors) {
                                if value.0 != 0.0 {
                                    continue;
                                }
                            }
                            self.execute_node(inst_index, node_id.clone());
                        }
                    }
                } else {
                    // Execute the tree which matches the current action, i.e. "onXXX", like "onMove"

                    let mut tree_id: Option<usize> = None;
                    if let Some(action) = &self.instances[inst_index].action {
                        for id in &self.instances[inst_index].tree_ids {
                            if let Some(behavior) = self.get_behavior(self.instances[inst_index].behavior_id, BehaviorType::Behaviors) {
                                if let Some(node) = behavior.data.nodes.get(&id) {
                                    if node.name == action.action {
                                        tree_id = Some(*id);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(tree_id) = tree_id {
                            self.execute_node(inst_index, tree_id);
                        } else {
                            println!("Cannot find valid tree for action {}", action.action);
                        }

                        self.instances[inst_index].action = None;
                    }
                }
            }

            // Add to the characters

            if let Some(position) = self.instances[inst_index].position {
                if let Some(tile) = self.instances[inst_index].tile {
                    let character = CharacterData { position,
                        tile,
                        name: self.instances[inst_index].name.clone(),
                        id: self.instances[inst_index].id
                     };
                     if let Some(list) = self.characters.get_mut(&position.0) {
                         list.push(character);
                     } else {
                         self.characters.insert(position.0, vec![character]);
                     }
                }
            }
        }

        // Execute region area behaviors
        let mut to_execute: Vec<(usize, usize)> = vec![];
        if let Some(region) = self.regions.get_mut(&self.curr_region_id) {
            region.displacements = HashMap::new();
            for area_index in 0..region.data.areas.len() {
                for (node_id, node) in &region.behaviors[area_index].data.nodes {
                    if node.behavior_type == BehaviorNodeType::InsideArea {
                        to_execute.push((area_index, *node_id));
                    }
                }
            }
        }
        for pairs in to_execute {
            self.execute_area_node(pairs.0, pairs.1);
        }

        // Parse the player characters and generate updates

        for inst_index in 0..self.instances.len() {
            if self.instances[inst_index].instance_type == BehaviorInstanceType::Player {

                let mut region        : Option<GameRegionData> = None;
                let mut characters    : Vec<CharacterData> = vec![];
                let mut displacements : HashMap<(isize, isize), (usize, usize, usize, TileUsage)> = HashMap::new();

                if let Some(position) = self.instances[inst_index].position {

                    if let Some(reg) = self.regions.get(&position.0) {
                        // Check if the character is in a region we did not send to the client yet
                        if self.instances[inst_index].regions_send.contains(&position.0) == false {
                            region = Some(reg.data.clone());
                            self.instances[inst_index].regions_send.insert(position.0);
                        }
                        // Copy the displacements
                        displacements = reg.displacements.clone();
                    }

                    // Send the characters of the client region
                    if let Some(chars) = self.characters.get(&position.0) {
                        characters = chars.clone();
                    }
                }

                let update = GameUpdate{
                    position: self.instances[inst_index].position,
                    tile: self.instances[inst_index].tile,
                    region,
                    displacements,
                    characters
                 };

                self.instances[inst_index].update = serde_json::to_string(&update).ok();
            }
        }

        // TODO REMOVE
        // Set game frame dimensions
        if let Some(size) = size {

            self.game_anim_counter = size.2;

            if self.game_frame.len() != self.game_screen_width * self.game_screen_height * 4 {
                self.game_frame = vec![0; self.game_screen_width * self.game_screen_height * 4];
            }

            // Execute the game logic behavior, this also draws into the game_frame
            if let Some(game_inst_index) = self.game_instance_index {

                if self.scopes.is_empty() == false {

                    if let Some(locked_tree) = self.instances[game_inst_index].locked_tree {
                        self.execute_game_node(game_inst_index, locked_tree);
                    }

                    for custom_scope_id in &mut self.custom_scopes_ordered {

                        if let Some(scope) = self.custom_scopes.get_mut(&custom_scope_id) {

                            // Get the messages
                            if let Some(mut messages) = scope.get_value::<ScriptMessages>("messages") {
                                //println!("{:?}", messages);
                                messages.clear();
                                scope.set_value("messages", messages);
                            }

                            // Get the draw commands
                            if let Some(mut draw) = scope.get_value::<ScriptDraw>("draw") {
                                //println!("{:?}", draw);

                                let game_frame = &mut self.game_frame[..];
                                let stride = self.game_screen_width;

                                for cmd in &draw.commands {

                                    if let Some(draw2d) = &self.draw2d {

                                        match cmd {
                                            ScriptDrawCmd::DrawRect(rect, rgb) => {
                                                draw2d.draw_rect(game_frame, &rect.rect, stride, &rgb.value);
                                            },
                                            ScriptDrawCmd::DrawText(pos, font_name, text, size, rgb) => {
                                                if let Some(font) = self.asset.as_ref().unwrap().game_fonts.get(font_name) {
                                                    draw2d.blend_text(game_frame, &pos.pos, stride, font, *size, text, &rgb.value);
                                                }
                                            },
                                            ScriptDrawCmd::DrawGame(rect, size) => {
                                                //draw2d.draw_rect(game_frame, &rect.rect, stride, &rgb.value);

                                                let region_id = self.regions_ids[0];

                                                if let Some(region) = self.regions.get(&region_id) {
                                                    // Find the behavior instance for the current behavior id
                                                    let mut inst_index = 0_usize;
                                                    let behavior_id = self.behaviors_ids[0];
                                                    for index in 0..self.instances.len() {
                                                        if self.instances[index].behavior_id == behavior_id {
                                                            inst_index = index;
                                                            break;
                                                        }
                                                    }

                                                    _ = self.draw2d.as_ref().unwrap().draw_region_centered_with_instances(game_frame, region, &rect.rect, inst_index, stride, *size as usize, self.game_anim_counter, &self.asset.as_ref().unwrap(), &self.instances);
                                                }
                                            },
                                            ScriptDrawCmd::DrawRegion(name, rect, size) => {
                                                for (index, n) in self.regions_names.iter().enumerate() {
                                                    if n == name {
                                                        if let Some(region) = self.regions.get(&self.regions_ids[index]) {

                                                            _ = self.draw2d.as_ref().unwrap().draw_region_content(game_frame, region, &rect.rect, stride, *size as usize, self.game_anim_counter, &self.asset.as_ref().unwrap());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                draw.clear();
                                scope.set_value("draw", draw);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Clear the game instances
    pub fn clear_instances(&mut self) {
        self.instances = vec![];
        self.scopes = vec![];
        self.ast = HashMap::new();
        self.executed_connections = vec![];
        self.changed_variables = vec![];
        self.active_instance_indices = vec![];
        self.player_ids_inst_indices = HashMap::new();
        self.custom_scopes = HashMap::new();
        self.custom_scopes_ordered = vec![];
        self.game_screens = HashMap::new();
    }

    /// Creates a new player instance and returns the index
    pub fn create_player_instance(&mut self, player_id: usize) {
        let index = self.create_behavior_instance(self.player_behavior_id);
        self.instances[index].instance_type = BehaviorInstanceType::Player;
        self.player_ids_inst_indices.insert(player_id, index);
    }

    /// Assign an action to an instance
    pub fn execute_packed_instance_action(&mut self, action: String) {
        if let Some(action) = serde_json::from_str::<PlayerAction>(&action).ok() {
            if let Some(index) = self.player_ids_inst_indices.get(&action.player_id) {
                self.instances[*index].action = Some(action);
            }
        }
    }

    pub fn startup(&mut self) {

        self.asset = Some(Asset::new());
        self.asset.as_mut().unwrap().load_from_path(self.path.clone());

        self.draw2d = Some(Draw2D {});

        self.create_behavior_instances();
        self.game_instance_index = Some(self.create_game_instance());
        self.create_player_instance(131313);
        self.activate_region_instances(self.regions_ids[0]);

        // Engine

        self.engine.register_type::<ScriptMessages>()
            .register_fn("debug", ScriptMessages::debug);

        self.engine.register_type::<ScriptDraw>()
            .register_fn("rect", ScriptDraw::rect)
            .register_fn("game", ScriptDraw::game)
            .register_fn("region", ScriptDraw::region)
            .register_fn("text", ScriptDraw::text);

        self.engine.register_type::<ScriptRect>()
            .register_fn("rect", ScriptRect::new);

        self.engine.register_type::<ScriptPosition>()
            .register_fn("pos2d", ScriptPosition::new);

        self.engine.register_type::<ScriptRGB>()
            .register_fn("rgb", ScriptRGB::new)
            .register_fn("rgba", ScriptRGB::new_with_alpha);

        //if let Some(game_inst_index) = self.game_instance_index {
        //    self.scopes[game_inst_index].set_value("messages", ScriptMessages::new());
        //    self.scopes[game_inst_index].set_value("draw", ScriptDraw::new());
        //}
    }

    pub fn shutdown(&mut self) {
        self.clear_instances();
        self.game_instance_index = None;
        self.asset = None;
        self.draw2d = None;
    }

    /// Locally poll a player update, this is used for local single player games
    pub fn poll_update(&mut self, player_id: usize) -> Option<String> {
        if let Some(index) = self.player_ids_inst_indices.get(&player_id) {
            return self.instances[*index].update.clone();
        }
        None
    }
}