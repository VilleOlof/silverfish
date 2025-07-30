# silverfish

![Silverfish Logo](./logo.webp)

Easily edit Minecraft worlds with a simple, fast and powerful API.  
Works with worlds from version **1.18+** (including modded worlds).  

With `silverfish` you can set blocks, retrive blocks.  
Even set and get biomes, all in an easy to use crate.  

You can either continue reading the documentation.  
Or look at some examples in `examples/*`.  

### Set block

When calling `Region::set_block`, it won't actually write the changes to the chunks.  
But instead write it to an internal buffer that also prevents duplicate blocks.  
*If a block is already present on some coordinates in the buffer, set_block returns a `None`*  
To actually flush the block changes to the chunks, call `Region::write_blocks`


```rust
use silverfish::Region;

let mut region = Region::full_empty((0, 0));
// look at `silverfish::Block` for info on blocks
region.set_block(42, 65, 84, "stone");
region.write_blocks()?;
let mut buf = vec![];
region.write(&mut buf)?;
```

### Get block

You can retrieve blocks in batches or single call.  
Use `Region::get_blocks` with a list of coordinates to batch them together.  


```rust
use silverfish::Region;

let mut region_buf = vec![];
let region = Region::from_region(&mut region_buf, (0, 0))?;
let block = region.get_block(42, 65, 84)?;
```

### Biomes

You can also set and or get biomes within your worlds.  
Each biome in your world, at the lowest level is divided into 4x4x4 cells.  
So to set a cell to a specific biome you have to specify:  
- The chunk coordinates  
- Section Y level  
- Cell within the section  

Or you can just call it with tuple: `(52, 12, 62)` with region local coordinates.  
As it implements `Into<BiomeCell>` and will convert it for you.  

```rust
// Set a biome cell
use silverfish::{Region, BiomeCell};

let mut region = Region::full_empty((0, 0));
region.set_biome(BiomeCell::new((4, 1), -1, (1, 1, 3)), "minecraft:plains");
region.write_biomes()?;
```

```rust
// Get a biome cell
use silverfish::Region;

let mut region = Region::full_empty((0, 0));
let biome = region.get_biome((61, 12, 284))?;
```

### Block properties

Blocks can have any property attached to them.  
The `Block` comes with both `new` & `new_with_props` and a `try` version that returns a result.  

```rust
use silverfish::Block;

let block = Block::try_new_with_props(
    "minecraft:sea_pickle", 
    &[("waterlogged", "true"), ("pickles", "3")]
)?;
```

Look futher down under `performance` for more information on block names and their namespaces.  

### Region

A `Region` is the main object you will work with to apply changes and read data.  
And can be constructed via 4 different methods.  
Note that the last argument for any `Region` constructor is the region coordinates.  

```rust
use silverfish::Region;

// A new empty region with no chunk data
let region = Region::empty(...);

// A new full region with empty pre-filled chunks
let region = Region::full_empty(...);

// Creates a region from a HashMap<(u8, u8), NbtCompound>
// Where each key is the chunk coordinate and the value is the entire chunk nbt compound
let region = Region::from_nbt(...);

// Creates a region based off a writer from a `.mca` region file format.  
let region = Region::from_region(...);
```

### Config

A config can be specified in the `Region` to dictate how it should write blocks.  
The most notable one is `update_lighting` which structures the chunks in so that  
Minecraft will automatically update the lighting in the chunks on first reload.  
*(which is set to true by default)*

```rust
use silverfish::{Config, Region};

let mut region = Region::full_empty((0, 0));
let config = Config {
    create_chunk_if_missing: false
    update_lighting: true,
};
region.config = config;
```

----

> [!NOTE]  
> Do note that all of these coordinates used above is local to the **region** *(x=0..512, z=0..512)*.  
> To transform normal *global* world coordinates to local region coordinates.  
> You can pass them through `silverfish::to_region_local`.  

## Performance

There is already lot of optimizations put into this crate to make it quite fast, if you ask me.  
But there is also some pitfalls and optimizations you,  
as the user can do to modify your world even faster than before.  

### Namespaced Blocks

When you construct a `Block`, you may notice that the id argument you give is `B: Into<Name>`.  
`Name` is an enum which can either be `Name::Namespaced` or `Name::Id`, this tells the block if it begins with  
a namespace or not. A namespace is the `minecraft:` part of an id (`minecraft:furnace`, for example).  
And if you know that your block has a namespace in it you can safetely construct a `Name` with a namespace.  

```rust
let name = Name::new_namespace("minecraft:bell");
let block = Block::try_new(name)?;
```

Namespaces are not strictly *required* but heavily recommended incase  
blocks would collide in let's say modded enviroments.  
If you create a new `Block` with just a `&str` it will default to an `Name::Id` .  
And automatically convert it to a namespaced variant if it doesn't contain a namespace on NBT write.  

### Pre-allocating internal buffers

If you already know which chunks and sections within your region  
that you will modify, it helps to preallocate those internal buffers.  
Since calling `region.set_block(...)` doesn't actually write the changes.  
We store the blocks in internal buffers until `region.write_blocks` is called.  

```rust
let mut region = Region::full_empty((0, 0));
// preallocates the first 16 chunks within the region
region.allocate_block_buffer(0..4, 0..4, -4..20, 4096);
region.set_block((6, 1, 7), "birch_planks");
// ...
```

Note that calling `allocate_block_buffer` or `allocate_biome_buffer`  
resets all internal buffers related to blocks.  
So if you've called `set_block` before preallocating, all of that is gone.  

If you need sparsed preallocation that isn't within a specific chunk range.  
Look at `region.set_block_buffer` & `region.set_biome_buffer` to manage it yourself.  

### Batching

Almost all operations you do can be made in batches internally.  

Let's say you're writing a ton of blocks.

#### set_block: Single calls, slow
```rust
let mut region = Region::full_empty((0, 0));
region.set_block(5, 1, 28, "air");
region.write_blocks()?;
region.set_block(71, -5, 14, "air");
region.write_blocks()?;
region.set_block(451, 51, 162, "air");
region.write_blocks()?;
```
#### set_block: Batched, fast
```rust
let mut region = Region::full_empty((0, 0));
region.set_block(5, 1, 28, "air");
region.set_block(71, -5, 14, "air");
region.set_block(451, 51, 162, "air");
region.write_blocks()?;
```

This can also be applied to getting a ton of blocks at the same time.  

#### get_block: Single calls, slow
```rust
let region = Region::full_empty((0, 0));
let block_1 = region.get_block(6, 1, 4)?;
let block_2 = region.get_block(1, -61, 52)?;
let block_3 = region.get_block(78, 13, 152)?;
let block_4 = region.get_block(4, 62, 84)?;
```
#### get_blocks: Batched, fast
```rust
let region = Region::full_empty((0, 0));
let blocks = region.get_blocks(vec![
    (6, 1, 4), (1, -61, 52),
    (78, 13, 152), (4, 62, 84)
])?;
```

### Set Sections

If you know that you will fill, let's say an entire region with a single block.  
It is over 10x times faster to do it via `region.set_sections`.  
This function and it's brother (`set_section`), allows you to set an entire  
section *(4096 blocks, 16\*16\*16)* to a single block at once.  

```rust
let mut region = Region::full_empty((0, 0));
let mut sections = Vec::with_capacity(24_576);
for x in 0..32 {
    for y in -4..20 {
        for z in 0..32 {
            sections.push(((x, z), y, "minecraft:air"));
        }
    }
}
region.set_sections(sections)?;
```

Look at the [Minecraft Wiki](https://minecraft.wiki/w/Chunk_format) for more information on how sections are structured.  

----

All of this batching also applies to biomes and their get / set.  

### Numbers

While these are pointless in real world examples.  
They are *fun*. 

On my machine (Ryzen 7 5800X) and in release mode.  
Have gotten a throughput of **10,168,009** blocks per second when writing to the chunks NBT.  

The scenario was writing *100,663,296* blocks (an entire region) that only contained the same block.  
So this got the maximum amount of palette caches hit and least clean up internally.  
This was also with the entire region preallocated within the internal buffers.  
And didn't use `set_section` or `set_sections` which is faster in real world use.  
Those 100 million or so blocks took *9s~* or so to flush from the buffers to NBT.  