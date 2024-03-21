use std::{borrow::Cow, collections::{hash_map::Keys, HashMap}};

use bevy::{ecs::system::SystemId, input::common_conditions::input_just_pressed, prelude::*};
use workarounds::next_state;

#[derive(Component)]
struct ConsoleTag;

#[derive(Component)]
pub struct ConsoleOutputTag;

#[derive(Component)]
struct ConsoleInputTag;

#[derive(States, Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ConsoleState {
    #[default]
    Closed,
    AnimatingOpen,
    AnimatingClosed,
    Open,
}

#[derive(States, Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum CmdTrigger{
    #[default]
    Ready,
    Fired
}

fn move_console(time:Res<Time<Real>>, mut style:Query<&mut Style, With<ConsoleTag>>, state:Res<State<ConsoleState>>, mut commands:Commands, mut evts:ResMut<Events<ReceivedCharacter>>) {
    const MOVE_SPEED:f32 = 100.0;
    let top = &mut style.single_mut().top;
    let mut pos = if let Val::Percent(pos) = top { *pos } else { -33.3 };
    pos += match **state {
        ConsoleState::AnimatingOpen => MOVE_SPEED*time.delta_seconds(),
        ConsoleState::AnimatingClosed => -MOVE_SPEED*time.delta_seconds(),
        _ => 0.0
    };
    *top = Val::Percent(pos);

    match **state {
        ConsoleState::AnimatingClosed => if pos <= -33.3 { 
            *top = Val::Percent(-33.3); 
            commands.insert_resource(NextState(Some(ConsoleState::Closed))) 
        },
        ConsoleState::AnimatingOpen => if pos >= 0.0 { 
            *top = Val::Percent(0.); 
            commands.insert_resource(NextState(Some(ConsoleState::Open)));
            evts.clear();
        },
        _ => ()
    }  
}

fn setup_ui(mut commands:Commands) {
    commands.spawn((ConsoleTag, NodeBundle{style:Style{
            position_type:PositionType::Absolute,
            display:Display::Flex,
            flex_direction:FlexDirection::Column,
            padding:UiRect::px(12., 12., 12., 0.0),
            width:Val::Percent(100.),
            top:Val::Percent(-33.3),
            height:Val::Percent(33.3),
            min_height:Val::Percent(33.3),
            max_height:Val::Percent(33.3),
            ..default()
        },
        background_color:BackgroundColor(Color::BLACK),
        ..default()
    })).with_children(|console|{
        console.spawn((ConsoleOutputTag, TextBundle{style:Style{width:Val::Percent(100.), height:Val::Percent(80.), min_height:Val::Percent(80.), ..default()}, text:Text::from_section("" , TextStyle::default()), ..default()}));
        console.spawn((ConsoleInputTag, TextBundle{style:Style{width:Val::Percent(100.), ..default()}, text:Text::from_sections(vec![TextSection{value:" > ".into(), ..default()}, TextSection::default(), TextSection{value:"|".into(), style:TextStyle::default()}]), ..default()}));
    });
}

/// A resource modified before the system call to include command line arguments meant for that system.
#[derive(Resource, Deref, DerefMut)]
pub struct CommandArgs(String);


fn text_input(
    mut evr_char: ResMut<Events<ReceivedCharacter>>,
    kbd: Res<ButtonInput<KeyCode>>,
    map:Res<CommandMap>,
    mut input_field:Query<&mut Text, (With<ConsoleInputTag>, Without<ConsoleOutputTag>)>,
    mut output_field:Query<&mut Text, (With<ConsoleOutputTag>, Without<ConsoleInputTag>)>,
    mut commands:Commands,
) {
    if kbd.just_pressed(KeyCode::Enter) {
        let cmd = std::mem::take(&mut input_field.single_mut().sections[1].value);
        let out = &mut output_field.single_mut().sections[0].value;
        out.push_str(cmd.as_str());
        out.push('\n');
        commands.insert_resource(CommandArgs(cmd));
        // Running cmd system in the next frame to make sure CommandArgs resource has been properly set. 
        commands.insert_resource(NextState(Some(CmdTrigger::Fired)))
    } else if kbd.just_pressed(KeyCode::Backspace) {
        input_field.single_mut().sections[1].value.pop();
    } else if kbd.just_pressed(KeyCode::Tab) {
        let input = input_field.single();
        let cmd_start = input.sections[1].value.as_str();
        let out = &mut output_field.single_mut().sections[0].value;
        let mut is_found_one = false;
        for cmd in (**map).keys() {
            if cmd.starts_with(cmd_start) {
                out.push_str(cmd);
                out.push(' ');
                is_found_one = true;
            }
        }
        if is_found_one {
            out.push('\n')
        } else {
            out.push_str("No commands start with that.\n");
        }
    } else if !kbd.just_pressed(KeyCode::Backquote) {
        for ev in evr_char.drain() {
            // ignore control (special) characters
            for char in ev.char.chars() {
                if !char.is_control() {
                    input_field.single_mut().sections[1].value.push(char);
                }
            }
        }
    }
}

fn run_cmd(cmd:Res<CommandArgs>, map: Res<CommandMap>, mut output_field:Query<&mut Text, (With<ConsoleOutputTag>, Without<ConsoleInputTag>)>, mut commands:Commands) {
    if let Some(call) = cmd.split(' ').next() {
        if let Some(id) = (**map).get(call) {
            commands.run_system(*id)
        } else {
            let console = &mut output_field.single_mut().sections[0].value;
            console.push_str("Command not found: ");
            console.push_str(call);
            console.push('\n')
        }
    }
}


#[derive(States, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
struct CommandLineCommandsTrigger(u16);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct CommandMap(HashMap<std::borrow::Cow<'static, str>, SystemId>);


/// Creates a drop down console that can be used to call one-shot systems
/// To add system as a callable command - use 
/// ```
/// # let app = App:new();
/// # fn your_system() {}
/// ConsolePlugin::add_cmd(&mut app, "run_me", your_system);
/// ```
/// The command arguments will be available to your system through [`CommandArgs`] resource.
/// Warning - the key "run_me" as well as [`SystemId`] generated from your system will be put in a public resource
/// so other plugins can mess with its keys and values, resuling in potentially unexpected system calls.
/// Inside the called system you can get the console output with
/// ```
/// mut out:Query<&mut Text, With<ConsoleOutputTag>>
/// ```
pub struct ConsolePlugin;
impl ConsolePlugin{
    pub fn add_cmd<M, S>(app:&mut App, name:impl Into<std::borrow::Cow<'static, str>>, system: S ) -> Option<SystemId>
where
    S: IntoSystem<(), (), M> + 'static,
    {
        let test = app.world.register_system(system);
        app.world.init_resource::<CommandMap>(); // Calling this just in case someone adds systems before registering the plugin.
        app.world.resource_mut::<CommandMap>().as_deref_mut().insert(name.into(), test)
    }
}

fn help(map:Res<CommandMap>, mut out:Query<&mut Text, With<ConsoleOutputTag>>) {
    let out = &mut out.single_mut().sections[0].value;
    out.push_str("Registered commands:\n");
    for cmd in (**map).keys() {
        out.push_str(cmd);
        out.push(' ');
    }
    out.push('\n')
}

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<ConsoleState>()
            .init_state::<CommandLineCommandsTrigger>()
            .init_state::<CmdTrigger>()
            .init_resource::<CommandMap>()
            .add_systems(Startup, setup_ui)
            .add_systems(Update, next_state(ConsoleState::AnimatingOpen).run_if(input_just_pressed(KeyCode::Backquote).and_then(in_state(ConsoleState::Closed))))
            .add_systems(Update, next_state(ConsoleState::AnimatingClosed).run_if(input_just_pressed(KeyCode::Backquote).and_then(in_state(ConsoleState::Open))))
            .add_systems(Update, move_console.run_if(in_state(ConsoleState::AnimatingClosed).or_else(in_state(ConsoleState::AnimatingOpen))))
            .add_systems(Update, text_input.run_if(in_state(ConsoleState::Open)))
            .add_systems(OnEnter(CmdTrigger::Fired), (next_state(CmdTrigger::Ready), run_cmd))
            ;
        Self::add_cmd(app, "help", help);
        // let mut map = HashMap::new();
        // for (idx, (call, system)) in self.callstate_map.iter().zip(self.systems.iter()).enumerate() {
        //     // app.add_systems(OnEnter(CommandLineCommandsTrigger((idx+1) as u16)), system);
        //     // app.add_schedule(system.clone());
        //     map.insert(call.clone(), (idx+1) as u16);
        // }
        // app.insert_resource(CommandMap(map));
    }
}