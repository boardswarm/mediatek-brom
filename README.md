# Mediatek bootrom protocol implementation

Mediatek bootroms implement a simple protocol that can be used over e.g. uart to, among other things, load a Download Agent (DA). This crate contains a sans-io protocol implementation and some small helper traits to build on top of sync or async io implementations.

For example to get the hardware code and version the following can be used over e.g. a sync serial transport implementation:
```rust,no_run
    use mediatek_brom::{io::BromExecute, Brom};
    # let mut transport = std::io::Cursor::new([0u8; 16]);
    let brom = transport.execute(Brom::handshake(0x201000)).unwrap();
    let hwcode = transport.execute(brom.hwcode()).unwrap();
    println!("Hwcode: {:x?}", hwcode);

```

## Credits

To understand the protocol the following open source implementations were
studied:
* [mtkclient](https://github.com/bkerler/mtkclient)
* [mtk_uartboot](https://github.com/981213/mtk_uartboot)
