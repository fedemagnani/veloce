Experiments with kernel bypass

- DPDK (dataplane development kit) is compatible just with Linux and some compatible NICs. 
- The recommended approach for development, is using a VM with a supported Linux distribution (on macOS, I used [UTM](https://mac.getutm.app/)).

### UTM setup
- Once downloaded, **create a virtualized VM** using the wizard:
  - memory should be 4096 MiB (4 GiB) minimum, with 8192 MiB (8 GiB) recommended for better performance.
  - cores 2-4 should be enough for development.
  - don't use display output (headless mode).
  - don't enable apple virtualization
  - Boot via Ubuntu 22.04 LTS ARM64 ISO file (download the server image [here](https://cdimage.ubuntu.com/releases/22.04/release/))
  - pick storage size (64Gib is fine)
  - Start the vm and configure it: the installation will probably fail, but dont worry
  - Stop the vm 
  - Clear the CD/DVD drive from the ISO image
  - Restart the vm with the login you specified


  <!-- - check "Open VM Settings" before saving the VM -->



<!-- 
- Now the VM should be ready, and you need to configure it to enable `net_virtio_user0` (the virtual NIC exposed to the guest OS):
    - Click QEMU in the left sidebar 
    - Click Arguments (requires macOS 12+)
    - Scroll to "QEMU Arguments" section 
    - Add these arguments in the "New…" field:
    ```
    -netdev type=vhost-user,id=net_virtio_user0,chardev=char_virtio_user0,vhostforce  
    -device virtio-net-pci,netdev=net_virtio_user0,bus=pci.0,addr=0x10  
    -chardev socket,id=char_virtio_user0,path=./vhost-user.sock,server   
    ```-->