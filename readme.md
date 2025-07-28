# rust-edit *(name pending)*

Easily edit Minecraft worlds with a simple, fast and powerful API.  
Works with worlds from version 1.18+ (including modded worlds).  

### Set block

When calling `Region::set_block`, it won't actually write the changes to the chunks.  
But instead write it to an internal buffer that also prevents duplicate blocks.  
*If a block is already present on some coordinates in the buffer, set_block returns a `None`*  
To actually flush the block changes to the chunks, call `Region::write_blocks`


```rust
use rust_edit::{Region, Block};

let mut region = Region::full_empty((0, 0));
region.set_block(42, 65, 84, Block::new("stone"));
region.write_blocks()?;
let mut buf = vec![];
region.write(&mut buf)?;
```

### Get block

You can retrieve blocks in batches or single call.  
Use `Region::get_blocks` with a list of coordinates to batch them together.  


```rust
use rust_edit::Region;

let mut region_buf = vec![];
let region = Region::from_region(&mut region_buf, (0, 0))?;
let block = region.get_block(42, 65, 84)?;
```

### Block properties

Blocks can have any property attached to them.  
The `Block` comes with both `new` & `new_with_props` and a `try` version that returns a result.  

```rust
use rust_edit::Block;

let block = Block::try_new_with_props(
    "minecraft:sea_pickle", 
    &[("waterlogged", "true"), ("pickles", "3")]
)?;
```

### Region

A `Region` is the main object you will work with to apply changes and read data.  
And can be constructed via 4 different methods.  
Note that the last argument for any `Region` constructor is the region coordinates.  

```rust
use rust_edit::Region;

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
use rust_edit::{Config, Region};

let mut region = Region::full_empty((0, 0));
let config = Config {
    create_chunk_if_missing: false
    update_lighting: true,
};
region.config = config;
```

----

Do note that all of these coordinates used above is local to the **region** *(x=0..512, z=0..512)*.  
To transform normal *global* world coordinates to local region coordinates.  
You can pass them through `rust_edit::to_region_local`.  
