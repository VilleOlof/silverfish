use std::collections::HashMap;

use crate::nbt::Block;

/// Config for when flushing operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushConfig {
    pub update_blocklight: bool,
    pub update_skylight: bool,
    pub light_mapping: HashMap<Block, u8>,
}

impl Default for FlushConfig {
    fn default() -> Self {
        Self {
            update_blocklight: true,
            update_skylight: true,
            light_mapping: FlushConfig::light_mapping_v_1_21_8(),
        }
    }
}

macro_rules! def_block {
    ($map:expr, $value:expr, $name:expr, [ $(($key:expr, $val:expr)),* $(,)? ]) => {
        $map.insert(
            Block::new_with_props(
                $name,
                [ $( ($key, $val) ),* ]
            ),
            $value,
        );
    };

    ($map:expr, $value:expr, $name:expr) => {
        $map.insert(
            Block::new($name),
            $value,
        );
    };
}

impl FlushConfig {
    /// Mapping blocks and their properties to a light level  
    ///
    /// Config based off light data for Minecraft 1.21.8
    pub fn light_mapping_v_1_21_8() -> HashMap<Block, u8> {
        let mut map = HashMap::new();

        // 15
        def_block!(map, 15, "beacon");
        def_block!(map, 15, "conduit");
        def_block!(map, 15, "end_gateway");
        def_block!(map, 15, "end_portal");
        def_block!(map, 15, "fire");
        def_block!(map, 15, "pearlescent_froglight");
        def_block!(map, 15, "verdant_froglight");
        def_block!(map, 15, "ochre_froglight");
        def_block!(map, 15, "glowstone");
        def_block!(map, 15, "jack_o_lantern");
        def_block!(map, 15, "lantern");
        def_block!(map, 15, "lava");
        def_block!(map, 15, "sea_lantern");
        def_block!(map, 15, "shroomlight");
        def_block!(map, 15, "campfire", [("lit", "true")]);
        def_block!(map, 15, "redstone_lamp", [("lit", "true")]);
        def_block!(map, 15, "respawn_anchor", [("charges", "4")]);
        def_block!(map, 15, "copper_bulb", [("lit", "true")]);
        def_block!(map, 15, "waxed_copper_bulb", [("lit", "true")]);

        // 14
        def_block!(map, 14, "cave_vines", [("berries", "true")]);
        def_block!(map, 14, "cave_vines_plant", [("berries", "true")]);
        def_block!(map, 14, "end_rod");
        def_block!(map, 14, "torch");
        def_block!(map, 14, "wall_torch");

        // 13
        def_block!(map, 13, "furnace", [("lit", "true")]);
        def_block!(map, 13, "blast_furnace", [("lit", "true")]);
        def_block!(map, 13, "smoker", [("lit", "true")]);

        // 12
        def_block!(map, 12, "vault", [("vault_state", "active")]);
        def_block!(map, 12, "exposed_copper_bulb", [("lit", "true")]);
        def_block!(map, 12, "waxed_exposed_copper_bulb", [("lit", "true")]);

        // 11
        def_block!(map, 11, "nether_portal");
        def_block!(map, 11, "respawn_anchor", [("charges", "3")]);

        // 10
        def_block!(map, 10, "crying_obsidian");
        def_block!(map, 10, "soul_campfire", [("lit", "true")]);
        def_block!(map, 10, "soul_fire");
        def_block!(map, 10, "soul_lantern");
        def_block!(map, 10, "soul_torch");
        def_block!(map, 10, "soul_wall_torch");

        // 9
        def_block!(map, 9, "redstone_ore", [("lit", "true")]);
        def_block!(map, 9, "deepslate_redstone_ore", [("lit", "true")]);

        // 8
        def_block!(map, 8, "trial_spawner", [("trial_spawner_state", "active")]);
        def_block!(map, 8, "weathered_copper_bulb", [("lit", "true")]);
        def_block!(map, 8, "waxed_weathered_copper_bulb", [("lit", "true")]);

        // 7
        def_block!(map, 7, "enchanting_table");
        def_block!(map, 7, "ender_chest");
        def_block!(map, 7, "glow_lichen");
        def_block!(map, 7, "redstone_torch", [("lit", "true")]);
        def_block!(map, 7, "redstone_wall_torch", [("lit", "true")]);
        def_block!(map, 7, "respawn_anchor", [("charges", "2")]);

        // 6
        def_block!(map, 6, "vault", [("vault_state", "inactive")]);

        // 5
        def_block!(map, 5, "amethyst_cluster");

        // 4
        def_block!(map, 4, "large_amethyst_bud");
        def_block!(map, 4, "oxidized_copper_bulb", [("lit", "true")]);
        def_block!(map, 4, "waxed_oxidized_copper_bulb", [("lit", "true")]);
        def_block!(
            map,
            4,
            "trial_spawner",
            [("trial_spawner_state", "waiting_for_players")]
        );
        def_block!(
            map,
            4,
            "trial_spawner",
            [("trial_spawner_state", "waiting_for_reward_ejection")]
        );

        // 3
        def_block!(map, 3, "magma_block");
        def_block!(map, 3, "respawn_anchor", [("charges", "1")]);

        // 2
        def_block!(map, 2, "medium_amethyst_bud");
        def_block!(map, 2, "firefly_bush");

        // 1
        def_block!(map, 1, "brewing_stand");
        def_block!(map, 1, "brown_mushroom");
        def_block!(map, 1, "calibrated_sculk_sensor");
        def_block!(map, 1, "dragon_egg");
        def_block!(map, 1, "end_portal_frame");
        def_block!(map, 1, "sculk_sensor");
        def_block!(map, 1, "small_amethyst_bud");

        // multi mappings
        let candles = vec![
            "candle",
            "white_candle",
            "orange_candle",
            "magenta_candle",
            "light_blue_candle",
            "yellow_candle",
            "lime_candle",
            "pink_candle",
            "gray_candle",
            "light_gray_candle",
            "cyan_candle",
            "purple_candle",
            "blue_candle",
            "brown_candle",
            "green_candle",
            "red_candle",
            "black_candle",
        ];
        for candle in candles {
            for lvl in 1..=4 {
                let candle_light = lvl * 3;
                map.insert(
                    Block::new_with_props(candle, [("candles", &lvl.to_string()), ("lit", "true")]),
                    candle_light,
                );
            }
        }

        // sea pickles
        for lvl in 1..=4 {
            let pickle_light = (lvl + 1) * 3;
            def_block!(
                map,
                pickle_light,
                "sea_pickles",
                [("pickles", &lvl.to_string()), ("waterlogged", "true")]
            );
        }

        for lvl in 0..=15 {
            def_block!(map, lvl, "light", [("level", &lvl.to_string())]);
        }

        map
    }
}
