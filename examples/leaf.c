int Leaf(int g, int h, int i, int j) {
    int f;
    f = (g + h) - (i + j);
    return f;
}

int main(){
    Leaf(1,1,1,1);
    return 0;
}
