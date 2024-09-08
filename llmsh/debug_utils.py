
def u8_to_str(vec_u8: list[int]):
    print("".join([chr(u) for u in vec_u8]))

def str_to_u8(s: str):
    print([ord(c) for c in s])
