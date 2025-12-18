#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv){
    if(argc <= 2){
        fprintf(stderr, "Usage: %s <file_or_rtsp_url>\n", argv[0]);
        return 1;
    }

    char cmd[1024];
    snprintf(cmd, sizeof(cmd),
             "%s/bin/python3 %s/extractors/mv_extractor.py \"%s\" \"%s\"",
             argv[5], argv[4], argv[1], argv[3]);

    printf("cmd: %s\n", cmd);
    return system(cmd);
}

